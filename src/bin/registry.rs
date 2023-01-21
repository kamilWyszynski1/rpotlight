use anyhow::{bail, Context};
use rpot::communication;
use rpot::fts;
use rpot::model;
use rpot::read;
use rpot::twoway;
use serde::Serialize;
use std::{collections::HashMap, path, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tonic::{transport::Server, Response, Status};
use tracing::warn;
use tracing::{error, info};

type Parsers = Arc<
    Mutex<
        HashMap<
            communication::ParserType,
            communication::parser_client::ParserClient<tonic::transport::Channel>,
        >,
    >,
>;

/// ParseContent and file_path merged into one struct.
/// It's not sent via rpc like that not to duplicate data.
#[derive(Debug, Clone, Default, Serialize)]
struct ParseContentWithPath {
    parse_content: model::Message,
    file_path: String,
}

impl fts::TokenProvider for ParseContentWithPath {
    fn get_tokens(&self) -> Vec<String> {
        self.parse_content.tokens.clone()
    }
}

pub struct Registry {
    /// Receives file paths that will be sent to parsers.
    tx: Option<mpsc::Receiver<String>>,

    /// Receives cli command.
    cli_tx: Option<twoway::Receiver<communication::CliRequest, communication::CliResponse>>,

    /// RPC clients for parsers.
    parsers: Parsers,

    /// Structure is responsible for all operation related to storing indexes and searches.
    fts: Arc<Mutex<fts::FTS<ParseContentWithPath>>>,
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
    fn new(
        tx: mpsc::Receiver<String>,
        cli_tx: twoway::Receiver<communication::CliRequest, communication::CliResponse>,
    ) -> Self {
        Self {
            tx: Some(tx),
            parsers: Arc::default(),
            cli_tx: Some(cli_tx),
            fts: Arc::default(),
        }
    }

    fn start_receiving(&mut self) {
        let mut tx = self.tx.take().unwrap();
        let parsers = self.parsers.clone();
        let fts = self.fts.clone();
        let mut cli_tx = self.cli_tx.take().unwrap();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // parsers things
                    file_path = tx.recv() => {
                        if file_path.is_none() {
                            warn!("file_path channel has been closed");
                            continue
                        }
                        match parse_file(file_path.unwrap(), parsers.clone()).await {
                            Ok(parsed) => {
                                let file_path = parsed.file_path.clone();
                                for parse_content in parsed.content {
                                    fts.lock().await.push(ParseContentWithPath { parse_content: parse_content.into(), file_path: file_path.clone() });
                                }
                            },
                            Err(err) => {
                                error!("could not parse file: {}", err)
                            },
                        }
                    }
                    // cli things
                    cmd = cli_tx.recv() => {
                        if cmd.is_none() {
                            warn!("cmd channel has been closed");
                            continue
                        }

                        let cmd = cmd.unwrap();
                        let content = cmd.content;

                        match communication::CliType::from_i32(cmd.c_type).unwrap() {
                            communication::CliType::Search => {
                                info!("fts content: {:?}", fts);
                                let r = fts.lock().await.search(content.clone());
                                let response: String = match r {
                                    Some::<Vec<ParseContentWithPath>>(found) => {
                                        serde_json::json!(found).to_string()
                                    },
                                    None => "not matches found".to_string()
                                };
                                if let Err(err) = cli_tx.response(communication::CliResponse { response }).await {
                                    error!(command = "search", content = content, err = err.to_string(),"failed to send response")
                                }
                            },
                        }
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

    if parser_type.is_none() {
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

struct CliServer {
    tx: Arc<Mutex<twoway::Sender<communication::CliRequest, communication::CliResponse>>>,
}

impl CliServer {
    fn new(tx: twoway::Sender<communication::CliRequest, communication::CliResponse>) -> Self {
        Self {
            tx: Arc::new(Mutex::new(tx)),
        }
    }
}

#[tonic::async_trait]
impl communication::cli_server::Cli for CliServer {
    async fn command(
        &self,
        request: tonic::Request<communication::CliRequest>,
    ) -> Result<Response<communication::CliResponse>, Status> {
        let response = self
            .tx
            .lock()
            .await
            .send(request.into_inner())
            .await
            .map_err(|e| {
                error!(err = e.to_string(), "error while sending cli request");
                Status::internal("error while sending cli request")
            })?
            .context("response is None")
            .map_err(|e| {
                error!(err = e.to_string(), "error occured");
                Status::internal("something went wrong")
            })?;
        Ok(Response::new(response))
    }
}

async fn read_and_send_files(tx: &mpsc::Sender<String>) {
    let mut files = vec![];

    info!("reading directory");

    read::visit_dirs(
        "/home/kamil/programming/rust/rpotlight",
        vec![read::IncludeOnly::Suffix(".rs".to_string())],
        vec![read::Exclude::Contains("target/debug".to_string())],
        |entry| files.push(entry.path().to_str().unwrap().to_string()),
    )
    .unwrap();

    info!("found {} files", files.len());

    for file in files {
        match tx.send(file).await {
            Ok(_) => {}
            Err(err) => {
                error!(error = err.0, "could not send message");
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let addr = "[::1]:50051".parse()?;
    let cli_addr = "[::1]:50053".parse()?;

    let (tx, rx) = mpsc::channel::<String>(1);
    let (cli_tx, cli_rx) = twoway::channel(100);
    let mut registry = Registry::new(rx, cli_rx);

    // spawn task that will read files
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::new(10, 0)).await;
            read_and_send_files(&tx).await;
        }
    });

    // spawn task that will receive cli rpc calls
    tokio::spawn(async move {
        Server::builder()
            .add_service(communication::cli_server::CliServer::new(CliServer::new(
                cli_tx,
            )))
            .serve(cli_addr)
            .await
            .unwrap();
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
