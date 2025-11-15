mod embed;
mod entities;
mod grpc;
mod logger;

use clap::Parser;
use futures::executor::block_on;
use log::warn;

use crate::{
    entities::string_convert::dynamic_import_embeddings, grpc::server::start_grpc_server,
    logger::get_logger_instance,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    import: Option<String>,

    #[arg(short, long)]
    start: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Import .env
    println!("Importing .env file");
    let _ = dotenvy::dotenv().map_err(|_| warn!("No .env file found"));

    let logger = get_logger_instance().await;
    log::set_boxed_logger(Box::new(logger))
        .map(|()| log::set_max_level(log::LevelFilter::Debug))
        .unwrap_or_else(|e| eprintln!("Failed to set logger: {}", e));

    // Start logger
    tokio::spawn(async {
        let logger = block_on(get_logger_instance());

        logger.periodic_flush_and_check().await;
    });

    println!("READY!");

    let args = Args::parse();

    if let Some(import) = args.import.as_deref() {
        dynamic_import_embeddings(import, args.start).await?;
        return Ok(());
    }

    // Start GRPC server
    start_grpc_server()
        .await
        .expect("gRPC server failed to start");

    Ok(())
}
