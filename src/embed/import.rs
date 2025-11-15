use super::{
    collections::EmbeddableEntityColumn, create::create_and_save_embeddings,
    errors::EmbeddingError, instances::get_db_instance,
};

use log::info;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait, Select,
};
use thiserror::Error;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::grpc::server::vecembed_rpc::VectorDbDocument;

fn string_to_i64(s: &str) -> i64 {
    match s.parse::<i64>() {
        Ok(num) => num, // If it's a number, return it directly
        Err(_) => {
            // If it's not a number, hash the string
            let mut hasher = DefaultHasher::new();
            s.hash(&mut hasher);
            let hash_result = hasher.finish() as i64;

            // Safely convert the u64 hash to i64
            hash_result % i64::MAX
        }
    }
}

#[derive(Error, Debug)]
pub enum ImportEmbeddingsError {
    #[error(transparent)]
    DbError(#[from] sea_orm::DbErr),

    #[error(transparent)]
    EmbeddingError(#[from] EmbeddingError),

    #[error("Unknown combination: {0}")]
    UnknownCombination(String),
}

pub const IMPORT_PAGE_SIZE: u64 = 100;
// 1 MB maximum chunk length
const MAX_TEXT_CHUNK_SIZE: usize = 1 * 1024 * 1024;
// 8 GB maximum mem limit
const MEM_LIMIT: usize = 8 * 1024 * 1024 * 1024;

pub async fn import_embeddings<E, C>(start_from: Option<u64>) -> Result<(), ImportEmbeddingsError>
where
    E: EmbeddableEntityColumn<E, C> + EntityTrait + Default + Send + Sync,
    C: ColumnTrait + Send + Sync,
    Select<E>: PaginatorTrait<'static, DatabaseConnection>,
{
    let db = get_db_instance().await;
    let default_entity = E::default();
    let entity_table_name = default_entity.table_name();
    let mut pages = E::find()
        .select_only()
        .column(E::primary_key_column())
        .apply_if(E::user_id_column(), |query, user_id_column| {
            query.column(user_id_column)
        })
        .apply_if(start_from, |query, start| {
            query.filter(E::primary_key_column().gte(start))
        })
        .filter(Expr::cust(
            "(qdrant_sync_at <> updated_at OR qdrant_sync_at IS NULL OR updated_at IS NULL)",
        ))
        .order_by_asc(E::order_by_column())
        .into_json()
        .paginate(db, IMPORT_PAGE_SIZE);

    while let Some(items) = pages.fetch_and_next().await? {
        let primary_key_column = E::primary_key_column().to_string();

        info!(
            "Conducting import for {} starting from ID: {}",
            primary_key_column, items[0]["id"]
        );
        let mut documents: Vec<VectorDbDocument> = Vec::new();
        let mut accumulated_size: usize = 0;

        for item in items {
            let primary_key_value = item[&primary_key_column].to_string();
            let mut start_pos = 1;
            let mut content = String::new();

            loop {
                let query = E::find()
                    .select_only()
                    .column_as(
                        Expr::cust(&format!(
                            "SUBSTRING({}, {}, {})",
                            E::text_column().to_string(),
                            start_pos,
                            MAX_TEXT_CHUNK_SIZE
                        )),
                        "content_chunk",
                    )
                    .filter(E::primary_key_column().eq(&primary_key_value))
                    .to_owned();

                let content_chunk: Option<String> = query
                    .into_json()
                    .one(db)
                    .await?
                    .map(|json| json["content_chunk"].to_string());

                if let Some(chunk) = content_chunk {
                    let chunk_len = chunk.len();
                    content.push_str(&chunk);
                    start_pos += MAX_TEXT_CHUNK_SIZE;

                    let mem_limit = std::env::var("MEM_LIMIT_MB")
                        .ok()
                        .and_then(|s| s.parse::<usize>().ok())
                        .map(|mb| mb * 1024 * 1024)
                        .unwrap_or(MEM_LIMIT);

                    if chunk_len < MAX_TEXT_CHUNK_SIZE || accumulated_size + chunk_len >= mem_limit {
                        let user_id: Option<u64> = item.get("user_id").and_then(|uid| uid.as_u64());

                        let document = VectorDbDocument {
                            id: string_to_i64(&primary_key_value),
                            table_name: entity_table_name.to_string(),
                            content: content.clone(),
                            user_id,
                        };
                        documents.push(document);
                        accumulated_size += content.len();
                        content.clear();

                        if accumulated_size >= mem_limit {
                            create_and_save_embeddings(documents).await?;
                            documents = Vec::new();
                            accumulated_size = 0;
                        }

                        if chunk_len < MAX_TEXT_CHUNK_SIZE {
                            break;
                        }
                    }
                } else {
                    break; // No more content
                }
            }
        }

        if !documents.is_empty() {
            create_and_save_embeddings(documents).await?;
        }
    }

    Ok(())
}
