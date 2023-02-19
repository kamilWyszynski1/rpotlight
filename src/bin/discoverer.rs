use rpot::discoverer::LocalDiscoverer;
use rpot::{communication, db};
use tonic::transport::Server;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let config = db::Config {
        app_name: Some("discoverer"),
        database: "discoverer",
        host: "localhost",
        password: "rpotlight",
        username: "rpotlight",
        port: 27017,
    };
    let conn = db::conn(config).await?;

    let addr = "[::1]:50059".parse()?;

    let ld = LocalDiscoverer::new_from_db(conn.collection("registered")).await?;
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
