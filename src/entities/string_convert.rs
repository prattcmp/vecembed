pub async fn dynamic_import_embeddings(
    entity_name: &str,
    start_from: Option<u64>,
) -> Result<(), crate::embed::import::ImportEmbeddingsError> {
    match entity_name {
        "contents" => {
            crate::embed::import::import_embeddings::<
                super::contents::Entity,
                super::contents::Column,
            >(start_from)
            .await
        }
        "uploaded_files" => {
            crate::embed::import::import_embeddings::<
                super::uploaded_files::Entity,
                super::uploaded_files::Column,
            >(start_from)
            .await
        }
        other => {
            Err(crate::embed::import::ImportEmbeddingsError::UnknownCombination(other.to_string()))
        }
    }
}
