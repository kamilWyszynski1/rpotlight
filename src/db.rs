use mongodb::{bson::doc, options::ClientOptions, Client, Database};

pub struct Config<'a> {
    pub app_name: Option<&'a str>,

    pub username: &'a str,
    pub password: &'a str,
    pub database: &'a str,
    pub host: &'a str,
    pub port: u32,
}

/// Returns new mongodb Database handle.
pub async fn conn(cfg: Config<'_>) -> anyhow::Result<Database> {
    // Parse your connection string into an options struct
    let mut client_options = ClientOptions::parse(format!(
        "mongodb://{}:{}@{}:{}",
        cfg.username, cfg.password, cfg.host, cfg.port,
    ))
    .await?;

    // Manually set an option
    client_options.app_name = cfg.app_name.map(|s| s.to_string());

    // Get a handle to the cluster
    let client = Client::with_options(client_options)?;
    // Ping the server to see if you can connect to the cluster
    let db = client.database(cfg.database);
    db.run_command(doc! {"ping": 1}, None).await?;

    Ok(db)
}
