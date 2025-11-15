use std::collections::HashMap;
use async_openai::types::CreateEmbeddingRequestArgs;
use qdrant_client::qdrant::{Condition, Filter, SearchParams, SearchPoints, SearchResponse};
use crate::embed::instances::{get_embedding_client_instance, MODEL_NAME};
use crate::grpc::server::vecembed_rpc::IdList;

use super::{
    collections::COLLECTION_NAME,
    errors::{EmbeddingError, QdrantClientError},
    instances::{get_qdrant_instance},
};

pub async fn get_documents(
    query: &str,
    task_description: &str,
    user_id: i64,
    filter_ids: HashMap<String, IdList>,
    limit: Option<u64>,
    params: Option<SearchParams>,
) -> Result<SearchResponse, EmbeddingError> {
    let client = get_qdrant_instance().await;
    let embedding_client = get_embedding_client_instance().await;


    let request = CreateEmbeddingRequestArgs::default()
        .model(MODEL_NAME)
        .input(vec![("Instruct: ").to_owned() + task_description + "\nQuery: " + query])
        .build()?;

    let response = embedding_client.embeddings().create(request).await?;

    // Collect all relevant external contents and user's uploaded files
    let mut content_conditions = vec![Condition::matches("table_name", "contents".to_string())];
    let mut uploaded_file_conditions = vec![
        Condition::matches("table_name", "uploaded_files".to_string()),
        Condition::matches("user_id", user_id),
    ];

    let mut should_filters = vec![];
    for (table_name, id_list) in &filter_ids {
        if table_name == "contents" && !id_list.ids.is_empty() {
            content_conditions.push(Condition::matches("id", id_list.ids.clone().into_iter().collect::<Vec<i64>>()));
            should_filters.push(Filter::must(content_conditions.clone()).into());
        }
        if table_name == "uploaded_files" && !id_list.ids.is_empty() {
            uploaded_file_conditions.push(Condition::matches("id", id_list.ids.clone().into_iter().collect::<Vec<i64>>()));
            should_filters.push(Filter::must(uploaded_file_conditions.clone()).into());

        }
    }

    let filter = if !should_filters.is_empty() {
        Some(Filter::should(should_filters))
    } else {
        Some(Filter::should([
            Filter::must(content_conditions).into(),
            Filter::must(uploaded_file_conditions).into(),
        ]))
    };

    Ok(client
        .search_points(
            SearchPoints {
                collection_name: COLLECTION_NAME.to_string(),
                vector: response.data[0].embedding.clone(),
                limit: limit.unwrap_or(100),
                with_payload: Some(vec!["document_id", "start", "end", "table_name"].into()),
                with_vectors: Some(false.into()),
                filter,
                params,
                ..Default::default()
            }
        )
        .await
        .map_err(QdrantClientError::from)?)
}
