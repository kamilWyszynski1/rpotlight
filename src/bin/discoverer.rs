use rpot::communication;
use rpot::discoverer::LocalDiscoverer;
use rpot::DB;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Server;

fn create_or_load_db<P: AsRef<Path>>(p: P) -> anyhow::Result<DB> {
    let tree = sled::open(p)?;
    Ok(Arc::new(Mutex::new(tree)))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let db = create_or_load_db("db")?;

    let addr = "[::1]:50059".parse()?;

    let ld = LocalDiscoverer::new(None, db);
    ld.keep_parsers_in_sync().await;

    let server_thread = tokio::spawn(async move {
        Server::builder()
            .add_service(communication::discoverer_server::DiscovererServer::new(ld))
            .serve(addr)
            .await
            .unwrap();
    });

    server_thread.await?;
    Ok(())
}
