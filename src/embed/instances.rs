use std::env;
use async_openai::Client;
use async_openai::config::OpenAIConfig;

use sea_orm::{Database, DatabaseConnection};
use tokenizers::{FromPretrainedParameters, PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};
use tokio::sync::OnceCell;
use crate::embed::errors::EmbeddingError::TokenizerError;
use qdrant_client::Qdrant;
use log::info;
use std::time::Duration;

static QDRANT_CLIENT_INSTANCE: OnceCell<Qdrant> = OnceCell::const_new();
static DB_POOL: OnceCell<DatabaseConnection> = OnceCell::const_new();
static TOKENIZER: OnceCell<Tokenizer> = OnceCell::const_new();
static EMBEDDING_CLIENT: OnceCell<Client<OpenAIConfig>> = OnceCell::const_new();

pub const MODEL_NAME: &str = "silatus/gte-Qwen2-7B-instruct-INT4";

pub async fn get_db_instance() -> &'static DatabaseConnection {
    DB_POOL
        .get_or_init(|| async {
            Database::connect(env::var("DATABASE_URL").expect("DB URL not set"))
                .await
                .expect("Couldn't connect to SQL DB")
        })
        .await
}

pub async fn get_tokenizer_instance() -> &'static Tokenizer {
    TOKENIZER
        .get_or_init(|| async {
            info!("Downloading tokenizer");
            let mut tokenizer = Tokenizer::from_pretrained("silatus/gte-Qwen2-7B-instruct-INT4", Some(FromPretrainedParameters {
                auth_token: std::env::var("HF_TOKEN").ok(),
                ..Default::default()
            }))
                .map_err(|e| TokenizerError(e.to_string())).expect("Tokenizer couldn't be loaded.");

            let padding_params = PaddingParams {
                strategy: PaddingStrategy::BatchLongest,
                pad_token: "<|endoftext|>".to_string(),
                pad_id: 151643,
                ..Default::default()
            };

            let truncation_params = TruncationParams {
                max_length: 32768 * 2,
                ..Default::default()
            };

            tokenizer
                .with_padding(Some(padding_params))
                .with_truncation(Some(truncation_params))
                .map_err(|e| TokenizerError(e.to_string())).unwrap();

            tokenizer
        })
        .await
}

pub async fn get_qdrant_instance() -> &'static Qdrant {
    QDRANT_CLIENT_INSTANCE.get_or_init(|| async {
        info!("Creating Qdrant Client...");

        let qdrant_url = env::var("QDRANT_CLIENT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string());
        let qdrant_api_key = env::var("QDRANT_API_KEY").ok();

        let config = qdrant_client::config::QdrantConfig {
            uri: qdrant_url,
            api_key: qdrant_api_key,
            timeout: Duration::from_secs(30),
            ..Default::default()
        };

        Qdrant::new(config).expect("Couldn't create Qdrant client")
    }).await
}

pub async fn get_embedding_client_instance() -> &'static Client<OpenAIConfig> {
    EMBEDDING_CLIENT
        .get_or_init(|| async {
            let config = OpenAIConfig::new()
                .with_api_key(env::var("OPENAI_API_KEY").unwrap_or("EMPTY".to_string()))
                .with_api_base(env::var("OPENAI_URL").unwrap_or("http://vecembed-model-service:8000/v1".to_string()));

            Client::with_config(config)
        }).await
}
