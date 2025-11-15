use async_openai::error::OpenAIError;
use thiserror::Error;
use qdrant_client::QdrantError;

#[derive(Error, Debug)]
pub enum QdrantClientError {
    #[error("Qdrant client error: {0}")]
    ClientError(String),
}

impl From<anyhow::Error> for QdrantClientError {
    fn from(err: anyhow::Error) -> Self {
        QdrantClientError::ClientError(err.to_string())
    }
}

impl From<QdrantError> for QdrantClientError {
    fn from(err: QdrantError) -> Self {
        QdrantClientError::ClientError(err.to_string())
    }
}

#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error(transparent)]
    QdrantClient(#[from] QdrantClientError),

    #[error(transparent)]
    DbError(#[from] sea_orm::DbErr),

    #[error(transparent)]
    OpenAIError(#[from] OpenAIError),

    #[error("Tokenizer failed: `{0}`")]
    TokenizerError(String),

    #[error(transparent)]
    TaskJoinError(#[from] tokio::task::JoinError),
}
