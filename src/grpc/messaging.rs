use qdrant_client::qdrant::{QuantizationSearchParams, SearchParams};
use tonic::{Request, Response, Status};

use crate::embed::create::create_and_save_embeddings;
use crate::embed::errors::EmbeddingError;
use crate::embed::get::get_documents;

use crate::grpc::server::vecembed_rpc::vec_embed_rpc_server::VecEmbedRpc;
use crate::grpc::server::vecembed_rpc::{
    DocumentReply, DocumentsReply, RetrieveDocumentsRequest, StoreVectorEmbeddingReply,
    StoreVectorEmbeddingRequest, StoreVectorEmbeddingsReply, StoreVectorEmbeddingsRequest,
};

impl From<EmbeddingError> for Status {
    fn from(err: EmbeddingError) -> Self {
        match err {
            EmbeddingError::QdrantClient(_) => Status::internal(format!("Qdrant Client: {}", err)),
            EmbeddingError::DbError(_) => Status::internal(format!("DB Error: {}", err)),
            EmbeddingError::OpenAIError(_) => Status::internal(format!("vLLM Server Error: {}", err)),
            EmbeddingError::TokenizerError(_) => Status::internal(format!("{}", err)),
            EmbeddingError::TaskJoinError(_) => Status::internal(format!("Task Join: {}", err)),
        }
    }
}

#[derive(Debug, Default)]
// Use this struct for GRPC implementations
pub struct VecEmbedService;
#[tonic::async_trait]
impl VecEmbedRpc for VecEmbedService {
    async fn store_vector_embedding(
        &self,
        request: Request<StoreVectorEmbeddingRequest>,
    ) -> Result<Response<StoreVectorEmbeddingReply>, Status> {
        let req = request.into_inner();
        if let Some(document) = req.document {
            let successful = create_and_save_embeddings(vec![document]).await?;
            let reply = StoreVectorEmbeddingReply { successful };

            return Ok(Response::new(reply));
        }

        Err(Status::invalid_argument("No document provided."))
    }

    async fn retrieve_documents(
        &self,
        request: Request<RetrieveDocumentsRequest>,
    ) -> Result<Response<DocumentsReply>, Status> {
        let req = request.into_inner();
        let documents = get_documents(
            &req.query,
            &req.task_description,
            req.user_id,
            req.filter_ids,
            req.limit,
            proto_to_search_params(req.params),
        )
        .await?
        .result
        .into_iter()
        .map(|scored_point| {
            let table_name = scored_point
                .payload
                .get("table_name")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();

            // Extracting the id and converting it to u64
            let id = scored_point
                .payload
                .get("document_id")
                .unwrap()
                .clone()
                .into_json()
                .as_u64()
                .unwrap();

            let start = scored_point
                .payload
                .get("start")
                .unwrap()
                .clone()
                .into_json()
                .as_u64()
                .unwrap();

            let end = scored_point
                .payload
                .get("end")
                .unwrap()
                .clone()
                .into_json()
                .as_u64()
                .unwrap();

            DocumentReply {
                table_name,
                id,
                user_id: req.user_id,
                ranking_score: scored_point.score,
                start,
                end,
            }
        })
        .collect();

        let reply = DocumentsReply { documents };
        Ok(Response::new(reply))
    }

    async fn store_vector_embeddings(
        &self,
        request: Request<StoreVectorEmbeddingsRequest>,
    ) -> Result<Response<StoreVectorEmbeddingsReply>, Status> {
        let req = request.into_inner();
        if !req.documents.is_empty() {
            let successful = create_and_save_embeddings(req.documents).await?;
            let reply = StoreVectorEmbeddingsReply { successful };

            return Ok(Response::new(reply));
        }

        Err(Status::invalid_argument("No documents provided."))
    }
}

impl From<QuantizationSearchParams>
    for crate::grpc::server::vecembed_rpc::QuantizationSearchParams
{
    fn from(params: QuantizationSearchParams) -> Self {
        crate::grpc::server::vecembed_rpc::QuantizationSearchParams {
            ignore: params.ignore,
            rescore: params.rescore,
            oversampling: params.oversampling,
        }
    }
}

fn proto_to_quantization_search_params(
    proto_params: Option<crate::grpc::server::vecembed_rpc::QuantizationSearchParams>,
) -> Option<QuantizationSearchParams> {
    proto_params.map(|params| QuantizationSearchParams {
        ignore: params.ignore,
        rescore: params.rescore,
        oversampling: params.oversampling,
    })
}

fn proto_to_search_params(
    proto_params: Option<crate::grpc::server::vecembed_rpc::SearchParams>,
) -> Option<SearchParams> {
    log::info!("hnsw_ef {:?}", proto_params.clone().unwrap().hnsw_ef);
    proto_params.map(|params| SearchParams {
        hnsw_ef: params.hnsw_ef,
        exact: params.exact,
        quantization: proto_to_quantization_search_params(params.quantization),
        indexed_only: params.indexed_only,
    })
}
