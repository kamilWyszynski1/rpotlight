use anyhow::{bail, Context};
use rpot::read;
use rpot::{communication, parse};
use std::{collections::HashMap, path, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tonic::{transport::Server, Response, Status};
use tracing::{error, info};

type Parsers = Arc<
    Mutex<
        HashMap<
            communication::ParserType,
            communication::parser_client::ParserClient<tonic::transport::Channel>,
        >,
    >,
>;

#[derive(Debug)]
pub struct Registry {
    tx: Option<mpsc::Receiver<String>>,
    parsers: Parsers,
}

#[tonic::async_trait]
impl communication::register_server::Register for Registry {
    async fn register(
        &self,
        request: tonic::Request<communication::RegisterRequest>,
    ) -> Result<Response<communication::RegisterResponse>, Status> {
        let data = request.into_inner();

        info!("trying to connect to parser server, {:?}", data);

        let client = communication::parser_client::ParserClient::connect(format!(
            "http://[::1]:{}",
            data.port
        ))
        .await
        .map_err(|e| {
            error!("error while creating client, {}", e);
            Status::internal("could not create client")
        })?;

        self.parsers.lock().await.insert(data.p_type(), client);

        Ok(Response::new(communication::RegisterResponse::default()))
    }
}

impl Registry {
    fn new(tx: mpsc::Receiver<String>) -> Self {
        Self {
            tx: Some(tx),
            parsers: Arc::default(),
        }
    }

    fn start_receiving(&mut self) {
        let mut local_tx = self.tx.take().unwrap();
        let local_parsers = self.parsers.clone();
        tokio::spawn(async move {
            tokio::select! {
                file_path = local_tx.recv() => {
                    if let Err(err) = parse_file(file_path.unwrap(), local_parsers).await {
                        error!("could not parse file: {}", err)
                    }
                }
            }
        });
    }
}

async fn parse_file(
    file_path: String,
    parsers: Parsers,
) -> anyhow::Result<communication::ParseResponse> {
    let parser_type = match path::Path::new(&file_path)
        .extension()
        .unwrap()
        .to_str()
        .unwrap()
    {
        "go" => Some(communication::ParserType::Golang),
        "rs" => Some(communication::ParserType::Rust),
        _ => None,
    };

    if let None = parser_type {
        bail!("could not find ParserType enum for {} file", file_path);
    }

    Ok(parsers
        .lock()
        .await
        .get_mut(&parser_type.unwrap())
        .context("no registered parser")?
        .parse(communication::ParseRequest { file_path })
        .await?
        .into_inner())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let addr = "[::1]:50051".parse()?;

    let (tx, rx) = mpsc::channel::<String>(1);
    let mut registry = Registry::new(rx);

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::new(10, 0)).await;
            let mut files = vec![];

            info!("reading directory");

            read::visit_dirs(
                "/Users/kamilwyszynski/private/rpotlight/src",
                vec![read::IncludeOnly::Suffix("rs".to_string())],
                vec![],
                |entry| files.push(entry.path().to_str().unwrap().to_string()),
            )
            .unwrap();

            info!("found {} files", files.len());

            for file in files {
                tx.send(file).await.unwrap();
            }
        }
    });

    registry.start_receiving();

    Server::builder()
        .add_service(communication::register_server::RegisterServer::new(
            registry,
        ))
        .serve(addr)
        .await?;

    Ok(())
}
