pub mod vecembed_rpc {
    tonic::include_proto!("vecembedrpc");
}

use std::time::Duration;
use tonic::transport::Server;
use vecembed_rpc::vec_embed_rpc_server::VecEmbedRpcServer;

use crate::grpc::messaging::VecEmbedService;

const DEFAULT_GRPC_PORT: &str = "60061";

pub async fn start_grpc_server() -> Result<(), Box<dyn std::error::Error>> {
    // Create gRPC server
    let port = std::env::var("GRPC_SERVER_PORT").unwrap_or(DEFAULT_GRPC_PORT.to_string());
    let addr = format!("0.0.0.0:{}", port).parse()?;
    log::info!("Starting gRPC server on port {}", port);
    let service = VecEmbedService;

    Server::builder()
        .timeout(Duration::from_secs(120))
        .tcp_keepalive(Some(Duration::from_secs(120)))
        .tcp_nodelay(true)
        .add_service(VecEmbedRpcServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
