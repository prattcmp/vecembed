use std::{
    collections::HashMap,
    sync::Arc,
};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

use chrono::{DateTime, Utc};
use futures::stream::{StreamExt};
use qdrant_client::{
    client::Payload,
    qdrant::{
        vectors_config::Config, Condition, CreateCollection,
        Distance, FieldType, Filter, PointStruct, VectorsConfig, CreateFieldIndexCollection, UpsertPoints, DeletePoints,
        VectorParams, PointsSelector, points_selector::PointsSelectorOneOf,
    },
};

use async_openai::{types::CreateEmbeddingRequestArgs};
use futures::future::join_all;
use qdrant_client::qdrant::{CreateFieldIndexCollectionBuilder, OptimizersConfigDiff};
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

use super::{
    errors::{EmbeddingError, QdrantClientError},
    instances::{get_qdrant_instance},
};

use crate::embed::{
    chunk_strings::StringChunkIterator,
    collections::COLLECTION_NAME,
    instances::{self, get_db_instance},
};
use crate::embed::instances::get_embedding_client_instance;
use crate::grpc::server::vecembed_rpc::VectorDbDocument;

const MAX_DOCUMENT_BATCH_SIZE: usize = 50;
const MAX_CHUNK_TEXT_LENGTH: usize = 25000;
const MAX_TEXT_CHUNK_BATCH_SIZE: usize = 64;

fn chunks_to_points(
    chunks: Vec<(Vec<f32>, usize, usize)>,
    payload: HashMap<&str, serde_json::Value>,
) -> Vec<PointStruct> {
    chunks
        .into_iter()
        .map(|(embedding, start, end)| {
            let mut final_payload_hashmap = HashMap::new();
            final_payload_hashmap.insert("start", serde_json::Value::from(start));
            final_payload_hashmap.insert("end", serde_json::Value::from(end));

            final_payload_hashmap.extend(payload.clone());

            // Combine hashmaps
            let json_payload: serde_json::Value =
                serde_json::to_value(final_payload_hashmap).unwrap();

            // Convert to Qdrant payload
            let payload: Payload = json_payload.try_into().unwrap();

            PointStruct::new(uuid::Uuid::now_v7().to_string(), embedding, payload)
        })
        .collect::<Vec<PointStruct>>()
}

async fn process_chunks(
    id: i64,
    table_name: &str,
    user_id: Option<u64>,
    chunks: Vec<(&str, usize, usize)>,
) -> Result<(), EmbeddingError> {
    let filtered_chunks: Vec<(&str, usize, usize)> = chunks
        .into_iter()
        .filter(|(s, _, _)| !s.is_empty())
        .collect();

    let embedding_client = Arc::new(get_embedding_client_instance().await);

    let chunk_embeddings = Arc::new(Mutex::new(Vec::new()));

    let max_text_chunk_batch_size = std::env::var("MAX_TEXT_CHUNK_BATCH_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(MAX_TEXT_CHUNK_BATCH_SIZE);

    let tasks = filtered_chunks.chunks(max_text_chunk_batch_size)
        .map(|chunk| {
            let embedding_client = Arc::clone(&embedding_client);
            let chunk_embeddings = Arc::clone(&chunk_embeddings);

            async move {
                let chunk_strings: Vec<String> = chunk
                    .iter()
                    .map(|(s, _, _)| s.to_string())
                    .collect();

                let request = CreateEmbeddingRequestArgs::default()
                    .model("silatus/gte-Qwen2-7B-instruct-INT4")
                    .input(chunk_strings)
                    .build()?;

                let response = embedding_client.embeddings().create(request).await?;

                let batch_embeddings: Vec<(Vec<f32>, usize, usize)> = chunk.iter()
                    .zip(response.data.iter())
                    .map(|((_, start, end), data)| (data.embedding.clone(), *start, *end))
                    .collect();

                let mut chunk_embeddings = chunk_embeddings.lock().await;
                chunk_embeddings.extend(batch_embeddings);

                Ok::<_, EmbeddingError>(())
            }
        })
        .collect::<Vec<_>>();

    // Execute all tasks concurrently
    let results = join_all(tasks).await;

    // Handle any errors
    for result in results {
        result?;
    }

    // Return the final result
    let chunk_embeddings = Arc::try_unwrap(chunk_embeddings)
        .expect("Mutex should be unwrapped")
        .into_inner();


    let client = get_qdrant_instance().await;

    // Process each document in the batch
    let embedding_size: u64 = chunk_embeddings[0].0.len() as u64;

    // Create Qdrant collection if one doesn't already exist
    let collection_exists = client
        .collection_exists(COLLECTION_NAME)
        .await
        .map_err(QdrantClientError::from)?;

    if !collection_exists {
        let data_threshold: u64 = 1 * 1000 * 1000;

        let create_collection_result = client
            .create_collection(
                CreateCollection {
                    collection_name: COLLECTION_NAME.to_string(),
                    vectors_config: Some(VectorsConfig {
                        config: Some(Config::Params(VectorParams {
                            size: embedding_size,
                            on_disk: Some(true),
                            distance: Distance::Cosine.into(),
                            ..Default::default()
                        })),
                    }),
                    optimizers_config: Some(OptimizersConfigDiff {
                        memmap_threshold: Some(data_threshold / 2),
                        indexing_threshold: Some(data_threshold / 2),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            )
            .await
            .map_err(QdrantClientError::from);

        // Handle specific error for existing collection
        if let Err(e) = create_collection_result {
            match e {
                QdrantClientError::ClientError(ref msg) if msg.contains("already exists") => {
                    // Ignore the error and continue if the collection already exists
                }
                _ => return Err(EmbeddingError::from(e)), // Propagate other errors
            }
        }

        // Create indexes
        client
            .create_field_index(
                CreateFieldIndexCollectionBuilder::new(
                    COLLECTION_NAME,
                    "document_id",
                    FieldType::Integer,
            ).wait(true)
            )
            .await
            .map_err(QdrantClientError::from)?;

        client
            .create_field_index(
                CreateFieldIndexCollectionBuilder::new(
                    COLLECTION_NAME,
                    "user_id",
                    FieldType::Integer,
                ).wait(true)
            )
            .await
            .map_err(QdrantClientError::from)?;

        client
            .create_field_index(
                CreateFieldIndexCollectionBuilder::new(
                    COLLECTION_NAME,
                    "table_name",
                    FieldType::Keyword,
                ).wait(true),
            )
            .await
            .map_err(QdrantClientError::from)?;
    }

    let mut payload_hashmap = HashMap::new();
    payload_hashmap.insert("table_name", serde_json::Value::from(table_name));
    payload_hashmap.insert(
        "model",
        serde_json::Value::from(instances::MODEL_NAME.to_string()),
    );
    payload_hashmap.insert("document_id", serde_json::Value::from(id));
    if let Some(user_id) = user_id {
        payload_hashmap.insert("user_id", serde_json::Value::from(user_id));
    }

    // Insert the data into the vector DB
    let points = chunks_to_points(chunk_embeddings.clone(), payload_hashmap.clone());
    let upsert_result = client
        .upsert_points(
            UpsertPoints {
                collection_name: COLLECTION_NAME.to_string(),
                points,
                ..Default::default()
            }
        )
        .await;

    if let Err(e) = upsert_result {
        if e.to_string().starts_with("status: NotFound") {
            // Retry the upsert operation after creating the collection
            let points = chunks_to_points(chunk_embeddings, payload_hashmap);
            client
                .upsert_points(
                    UpsertPoints {
                        collection_name: COLLECTION_NAME.to_string(),
                        points,
                        ..Default::default()
                    }
                )
                .await
                .map_err(QdrantClientError::from)?;
        }

        // For any other error, convert and return it
        return Err(EmbeddingError::from(QdrantClientError::from(e)));
    }

    let db = get_db_instance().await;

    let now: DateTime<Utc> = Utc::now();

    // Format the DateTime as a string for MySQL
    let formatted_now = now.format("%Y-%m-%d %H:%M:%S").to_string();

    let sql = format!(
        "UPDATE {} SET updated_at = ?, qdrant_sync_at = ? WHERE id = ?;",
        table_name
    );
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::MySql,
        sql,
        [
            formatted_now.clone().into(),
            formatted_now.into(),
            id.into(),
        ],
    ))
    .await?;
    Ok(())
}

pub async fn create_and_save_embeddings(
    documents: Vec<VectorDbDocument>,
) -> Result<bool, EmbeddingError> {
    let max_length = 8192;

    let max_document_batch_size = std::env::var("MAX_DOCUMENT_BATCH_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(MAX_DOCUMENT_BATCH_SIZE);

    let available_parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(max_document_batch_size);
    let documents_chunk_size = (documents.len() / available_parallelism).max(1);

    let documents = Arc::new(Mutex::new(documents));
    let qdrant_client = get_qdrant_instance().await;
    let collection_exists = Arc::new(AtomicBool::new(
        qdrant_client
            .collection_exists(COLLECTION_NAME)
            .await
            .map_err(QdrantClientError::from)?
    ));

    while {
        let docs = documents.lock().await;
        !docs.is_empty()
    } {
        let documents_chunk = {
            let mut docs = documents.lock().await;
            let docs_len = docs.len();
            docs.drain(..std::cmp::min(documents_chunk_size, docs_len))
                .collect::<Vec<_>>()
        };

        // Check if collection exists
        if collection_exists.load(Ordering::SeqCst) {
            let document_ids: Vec<i64> = documents_chunk.iter().map(|doc| doc.id).collect();
            qdrant_client
                .delete_points(
                    DeletePoints {
                        collection_name: COLLECTION_NAME.to_string(),
                        points: Some(PointsSelector {
                            points_selector_one_of: Some(PointsSelectorOneOf::Filter(Filter::must([
                                Condition::matches("document_id", document_ids),
                            ]))),
                        }),
                        ..Default::default()
                    }
                )
                .await
                .map_err(QdrantClientError::from)?;
        }

        // Iterate over each document in the chunk of documents
        let chunk_tasks: Vec<_> = documents_chunk
            .into_iter()
            .filter(|document| !document.content.is_empty())
            .map(|document| {
                let collection_exists = Arc::clone(&collection_exists);
                async move {
                    let mut chunk_iterator =
                        StringChunkIterator::new(&document.content, max_length);
                    let mut chunks: Vec<(&str, usize, usize)> = Vec::new();
                    let mut combined_length = 0;

                    while let Some(chunk) = chunk_iterator.next().await {
                        let chunk = chunk?;

                        let max_chunk_text_length = std::env::var("MAX_CHUNK_TEXT_LENGTH")
                            .ok()
                            .and_then(|s| s.parse::<usize>().ok())
                            .unwrap_or(MAX_CHUNK_TEXT_LENGTH);

                        if combined_length + chunk.0.len() > max_chunk_text_length && !chunks.is_empty() {
                            process_chunks(
                                document.id,
                                &document.table_name,
                                document.user_id,
                                chunks.clone(),
                            ).await?;
                            chunks.clear();
                            combined_length = 0;
                            collection_exists.store(true, Ordering::SeqCst);
                        }
                        chunks.push(chunk);
                        combined_length += chunk.0.len();
                    }

                    if !chunks.is_empty() {
                        process_chunks(
                            document.id,
                            &document.table_name,
                            document.user_id,
                            chunks,
                        ).await?;
                        collection_exists.store(true, Ordering::SeqCst);
                    }
                    Ok::<(), EmbeddingError>(())
                }
            })
            .collect();

        for task in chunk_tasks {
            task.await?;
        }
    }

    Ok(true)
}