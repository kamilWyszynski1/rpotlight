use crate::communication;
use crate::communication::ParseResponse;
use crate::fts;
use crate::model::ParseContentWithPath;
use crate::read;
use crate::read::IncludeOnly;
use crate::twoway;
use crate::watcher;
use anyhow::{bail, Context};
use bson::doc;
use futures::TryStreamExt;
use mongodb::Collection;
use std::io;
use std::path::Path;
use std::{collections::HashMap, path, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tracing::debug;
use tracing::warn;
use tracing::{error, info};

/// Type wrap for storing parses grpc clients.
pub type Parsers = Arc<
    Mutex<
        HashMap<
            communication::ParserType,
            communication::parser_client::ParserClient<tonic::transport::Channel>,
        >,
    >,
>;

type DiscovererClient =
    communication::discoverer_client::DiscovererClient<tonic::transport::Channel>;

#[derive(Debug, Clone, PartialEq)]
pub enum RegistryMessage {
    /// Orders file parse, registry will call registered parsers for that.
    Parse { file_path: String, new: bool },
    /// Removes parsed file from registry's fts. Cause by e.g. file deletion.
    Remove(String),
}

impl RegistryMessage {
    pub fn is_remove(&self) -> bool {
        if let Self::Remove(_) = self {
            return true;
        }
        false
    }
}

/// Structure handles all functionalities related to an app.
/// Various parsers can register here so that specific files will be handled by them.
/// Registry listens for cli and internal messages for communication.
/// It manages database state as well.
pub struct Registry {
    /// Receives file paths that will be sent to parsers.
    rx: Option<mpsc::Receiver<RegistryMessage>>,

    /// Receives cli command.
    cli_tx: Option<twoway::Receiver<communication::CliRequest, communication::CliResponse>>,

    /// RPC clients for parsers.
    //TODO: allow to register multiple parser of the same type, loadbalancing?
    parsers: Parsers,

    /// Structure is responsible for all operation related to storing indexes and searches.
    fts: Arc<Mutex<fts::FTS<ParseContentWithPath>>>,

    /// Database will store information about parsed files.
    db: Collection<ParseResponse>,
}

impl Registry {
    pub async fn new(
        rx: mpsc::Receiver<RegistryMessage>,
        parsers: Parsers,
        cli_tx: twoway::Receiver<communication::CliRequest, communication::CliResponse>,
        db: Collection<ParseResponse>,
    ) -> anyhow::Result<Self> {
        let fts = load_fts_from_db(&db).await?;
        Ok(Self {
            rx: Some(rx),
            parsers,
            cli_tx: Some(cli_tx),
            fts: Arc::new(Mutex::new(fts)),
            db,
        })
    }

    pub async fn start_receiving(&mut self) -> anyhow::Result<()> {
        let mut rx = self.rx.take().context("tx is not set")?;
        let parsers = self.parsers.clone();
        let fts = self.fts.clone();
        let mut cli_tx = self.cli_tx.take().context("cli_tx is not set")?;
        let db = self.db.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // parsers things
                    message = rx.recv() => match message {
                        Some(message) => match message {
                            RegistryMessage::Parse { file_path, new } => match parse_file(file_path.clone(), parsers.clone()).await {
                                Ok(parsed) => {
                                    info!(file_path = &file_path, new = new, "received parse message");

                                    // write to the db
                                    if let Err(err) = db.insert_one(parsed.clone(), None).await {
                                        error!(err= err.to_string(),file_path = file_path, new = new, "could not save parsed file to db");
                                    }

                                    // clear fts before writing (possibility) data of already stored and modified file
                                    let file_path = parsed.file_path.clone();
                                    let mut locked_fts =  fts.lock().await;

                                    locked_fts.delete(&file_path);

                                    for parse_content in parsed.content {
                                        locked_fts.push(ParseContentWithPath { message: parse_content.into(), file_path: file_path.clone() });
                                    }
                                },
                                Err(err) => {
                                    error!(file_path = file_path, new = new, "could not parse file: {}", err)
                                },
                            },
                            RegistryMessage::Remove(file_path) => {
                                info!(file_path = &file_path, "received remove message");

                                match fts.lock().await.delete(&file_path) {
                                    Some(_) => info!(id = file_path, "deleted from fts"),
                                    None => info!(id=file_path, "could not delete from fts"),
                                }
                                if let Err(err) = db.delete_one(doc!{ "file_path": file_path}, None).await {
                                    error!(method = "start_receiving", err = err.to_string(), "could not delete from db on RegistryMessage:Remove")
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

    /// Periodically calls discoverer service for current state of working parsers.
    pub async fn fetch_parsers(&self, mut discoverer_client: DiscovererClient) {
        let parsers = self.parsers.clone();

        tokio::spawn(async move {
            loop {
                for p_type in [communication::ParserType::Rs, communication::ParserType::Go] {
                    match discoverer_client
                        .discover(communication::DiscoverRequest {
                            p_type: p_type.into(),
                        })
                        .await
                    {
                        Ok(response) => {
                            for url in response.into_inner().urls {
                                match communication::parser_client::ParserClient::connect(url).await
                                {
                                    Ok(client) => {
                                        debug!(p_type = p_type.as_str_name(), "saving client");
                                        parsers.lock().await.insert(p_type, client);
                                    }
                                    Err(err) => error!(
                                        p_type = p_type.as_str_name(),
                                        err = err.to_string(),
                                        "could not create client"
                                    ),
                                }
                            }
                        }
                        Err(err) => {
                            if err.code() == tonic::Code::NotFound {
                                debug!(
                                    p_type = p_type.as_str_name(),
                                    "could not find registered parsers"
                                );
                            } else {
                                error!(err = err.to_string(), "could not call discoverer service")
                            }
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::new(3, 0)).await;
            }
        });
    }
}

/// Reads all parsed data from database and creates new FTS from that data.
async fn load_fts_from_db(
    db: &Collection<ParseResponse>,
) -> anyhow::Result<fts::FTS<ParseContentWithPath>> {
    // load database content at start
    let mut fts = fts::FTS::default();

    let mut found = db.find(None, None).await?;
    while let Some(value) = found.try_next().await? {
        // TODO: check file's checksum
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

/// Reads all parsed data from database and for each entry creates watcher.
/// Watcher is needed in case of already parsed, stored files that will be deleted later on.
async fn load_watcher_manager_from_db(
    db: &Collection<String>,
    tx: mpsc::Sender<RegistryMessage>,
) -> anyhow::Result<watcher::WatcherManager> {
    // load database content at start
    let manager = watcher::WatcherManager::default();

    let mut found = db.find(None, None).await?;

    while let Some(file_path) = found.try_next().await? {
        if let Err(err) = manager
            .watcher_proxy(Path::new(&file_path), tx.clone())
            .await
        {
            if let Some(nerr) = err.downcast_ref::<notify::Error>() {
                if let notify::ErrorKind::Io(ref io) = nerr.kind {
                    if io::ErrorKind::NotFound == io.kind() {
                        // finally we are sure that file was removed
                        tx.send(RegistryMessage::Remove(file_path.clone())).await?;
                        continue;
                    }
                }
            }
            error!(
                err = err.to_string(),
                method = "load_watcher_manager_from_db",
                "unknown error"
            )
        }
    }

    Ok(manager)
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
        "go" => Some(communication::ParserType::Go),
        "rs" => Some(communication::ParserType::Rs),
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
    db: Collection<String>,

    /// Sender for notifing Registry about changes.
    tx: mpsc::Sender<RegistryMessage>,

    /// Manager handles watchers, notifications and shutdown when file is deleted.
    watcher_manager: watcher::WatcherManager,
}

impl FileInfoProvider {
    pub async fn new(
        db: Collection<String>,
        tx: mpsc::Sender<RegistryMessage>,
    ) -> anyhow::Result<Self> {
        let watcher_manager = load_watcher_manager_from_db(&db, tx.clone()).await?;

        Ok(Self {
            db,
            tx,
            watcher_manager,
        })
    }

    async fn spawn_watcher(&self, path: &Path) -> anyhow::Result<()> {
        self.watcher_manager
            .watcher_proxy(path, self.tx.clone())
            .await
    }

    pub async fn read_and_send_files<P: AsRef<Path>>(&self, parsers: Parsers, path: P) {
        let path = path.as_ref();

        loop {
            tokio::time::sleep(std::time::Duration::new(10, 0)).await;
            let mut files = vec![];

            let available_parsers: Vec<IncludeOnly> = parsers
                .lock()
                .await
                .keys()
                .map(|pt| pt.as_str_name().to_lowercase())
                .map(|ext| format!(".{}", ext))
                .map(IncludeOnly::Suffix)
                .collect();
            if available_parsers.is_empty() {
                warn!("there's no registered parsers, skip reading files");
            }

            if let Err(err) = read::visit_dirs(
                path,
                available_parsers,
                vec![read::Exclude::Contains("target/debug".to_string())],
                |entry| files.push(entry.path().to_str().unwrap().to_string()),
            ) {
                error!(
                    error = err.to_string(),
                    method = "read_and_send_files",
                    "could not visit dirs"
                );
                continue;
            }

            info!("found {} files", files.len());

            for file_path in files {
                let found = self
                    .db
                    .find_one(doc! { "file_path": &file_path}, None)
                    .await?.and_then(f)

                if found.is_some() {
                    debug!(file = file_path.clone(), "file was already sent to process");
                    continue;
                }
                match self
                    .tx
                    .send(RegistryMessage::Parse {
                        file_path: file_path.clone(),
                        new: true,
                    })
                    .await
                {
                    Ok(_) => {
                        self.db.insert_one(&file_path, None).await?;
                        self.spawn_watcher(Path::new(&file_path)).await?;
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
