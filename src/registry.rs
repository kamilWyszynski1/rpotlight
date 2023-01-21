use crate::communication;
use crate::communication::ParseResponse;
use crate::fts;
use crate::model::ParseContentWithPath;
use crate::read;
use crate::twoway;
use crate::watcher;
use anyhow::{bail, Context};
use prost::Message;
use std::path::Path;
use std::{collections::HashMap, path, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tonic::{transport::Server, Response, Status};
use tracing::warn;
use tracing::{error, info};

/// Type wrap for storing parses grpc clients.
type Parsers = Arc<
    Mutex<
        HashMap<
            communication::ParserType,
            communication::parser_client::ParserClient<tonic::transport::Channel>,
        >,
    >,
>;

/// Type wrap for database.
pub type DB = Arc<Mutex<sled::Db>>;

#[derive(Debug, Clone, PartialEq)]
pub enum RegistryMessage {
    /// Orders file parse, registry will call registered parsers for that.
    Parse(String),
    /// Removes parsed file from registry's fts. Cause by e.g. file deletion.
    Remove(String),
}

pub struct Registry {
    /// Receives file paths that will be sent to parsers.
    rx: Option<mpsc::Receiver<RegistryMessage>>,

    /// Receives cli command.
    cli_tx: Option<twoway::Receiver<communication::CliRequest, communication::CliResponse>>,

    /// RPC clients for parsers.
    parsers: Parsers,

    /// Structure is responsible for all operation related to storing indexes and searches.
    fts: Arc<Mutex<fts::FTS<ParseContentWithPath>>>,

    /// Database will store information about prased files.
    db: DB,
}

impl Registry {
    pub async fn new(
        rx: mpsc::Receiver<RegistryMessage>,
        cli_tx: twoway::Receiver<communication::CliRequest, communication::CliResponse>,
        db: DB,
    ) -> anyhow::Result<Self> {
        let fts = load_fts_from_db(&db).await?;
        Ok(Self {
            rx: Some(rx),
            parsers: Arc::default(),
            cli_tx: Some(cli_tx),
            fts: Arc::new(Mutex::new(fts)),
            db,
        })
    }

    pub async fn start_receiving(&mut self) -> anyhow::Result<()> {
        let mut tx = self.rx.take().context("tx is not set")?;
        let parsers = self.parsers.clone();
        let fts = self.fts.clone();
        let mut cli_tx = self.cli_tx.take().context("cli_tx is not set")?;
        let parsed_tree = self.db.clone().lock().await.open_tree("parsed")?;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // parsers things
                    message = tx.recv() => match message {
                        Some(message) => match message {
                            RegistryMessage::Parse(file_path) => match parse_file(file_path.clone(), parsers.clone()).await {
                                Ok(parsed) => {
                                    // convert parsed, proto message into bytes
                                    let mut buf = vec![];
                                    parsed.encode(&mut buf).unwrap();
                                    // write those bytes
                                    parsed_tree.insert(file_path.clone(), buf).unwrap();

                                    let file_path = parsed.file_path.clone();
                                    for parse_content in parsed.content {
                                        fts.lock().await.push(ParseContentWithPath { message: parse_content.into(), file_path: file_path.clone() });
                                    }
                                },
                                Err(err) => {
                                    error!("could not parse file: {}", err)
                                },
                            },
                            RegistryMessage::Remove(file_path) => {
                                if let Some(_) = fts.lock().await.delete(file_path.clone()) {
                                    info!(id = file_path, "deleted from fts");
                                }
                            },
                        },
                        None => info!(method = "start_receiving", "tx channel is probably closed"),
                    },
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
        Ok(())
    }
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

async fn load_fts_from_db(db: &DB) -> anyhow::Result<fts::FTS<ParseContentWithPath>> {
    // load database content at start
    let mut fts = fts::FTS::default();
    let parsed_tree = db.clone().lock().await.open_tree("parsed")?;
    for msg in parsed_tree.into_iter() {
        let msg = msg?;
        let (_, value) = msg;

        let value = ParseResponse::decode(value.to_vec().as_slice())?;

        let file_path = value.file_path.clone();
        for parse_content in value.content {
            fts.push(ParseContentWithPath {
                message: parse_content.into(),
                file_path: file_path.clone(),
            });
        }
    }
    Ok(fts)
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

pub struct FileInfoProvider {
    db: DB,
}

impl FileInfoProvider {
    pub fn new(db: DB) -> Self {
        Self { db }
    }

    pub async fn read_and_send_files<P: AsRef<Path>>(
        &self,
        path: P,
        tx: mpsc::Sender<RegistryMessage>,
    ) -> anyhow::Result<()> {
        let path = path.as_ref();

        loop {
            tokio::time::sleep(std::time::Duration::new(10, 0)).await;
            let mut files = vec![];

            info!("reading directory");

            read::visit_dirs(
                path,
                vec![read::IncludeOnly::Suffix(".rs".to_string())],
                vec![read::Exclude::Contains("target/debug".to_string())],
                |entry| files.push(entry.path().to_str().unwrap().to_string()),
            )?;

            info!("found {} files", files.len());

            // separate tree for send information.
            let send_tree = self.db.lock().await.open_tree("send")?;

            for file in files {
                let was_sent = send_tree.get(file.clone())?;
                if was_sent.is_some() {
                    info!(file = file.clone(), "file was already sent to process");
                    continue;
                }
                let c = tx.clone();
                match tx.send(RegistryMessage::Parse(file.clone())).await {
                    Ok(_) => {
                        send_tree.insert(file.clone(), b"true")?;
                        tokio::spawn(async move {
                            if let Err(err) =
                                watcher::create_watcher(Path::new(&file).as_ref(), c).await
                            {
                                error!(
                                    err = err.to_string(),
                                    method = "read_and_send_files",
                                    "could not create file watcher"
                                )
                            }
                        });
                    }
                    Err(err) => {
                        error!(
                            error = err.to_string(),
                            method = "read_and_send_files",
                            "could not send message"
                        );
                    }
                }
            }
        }
    }
}
