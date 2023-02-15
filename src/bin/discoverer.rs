use rpot::communication;
use rpot::discoverer::{LocalDiscoverer, Tree, DISCOVERER_TREE};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Server;
use tracing::info;

fn create_or_load_db<P: AsRef<Path>>(p: P) -> anyhow::Result<Tree> {
    let tree = sled::open(p)?.open_tree(DISCOVERER_TREE)?;
    Ok(Arc::new(Mutex::new(tree)))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let db = create_or_load_db("db")?;

    let addr = "[::1]:50059".parse()?;

    let ld = LocalDiscoverer::new_from_db(db).await?;
    ld.keep_parsers_in_sync().await;

    let server_thread = tokio::spawn(async move {
        Server::builder()
            .add_service(communication::discoverer_server::DiscovererServer::new(ld))
            .serve(addr)
            .await
            .unwrap();
    });

    info!(addr = addr.to_string(), "server started");

    server_thread.await?;
    Ok(())
}
