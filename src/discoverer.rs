use crate::{
    communication::{self, parser_client::ParserClient},
    GRPCResult,
};
use anyhow::Context;
use futures::TryStreamExt;
use mongodb::{bson::doc, Collection};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Channel, Response, Status};
use tracing::{error, info, warn};

/// Represents registered parser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserModel {
    _id: bson::uuid::Uuid,
    host: String,
    port: u32,
    parser_type: communication::ParserType, // maps to 'communication::ParserType'.
}

pub const DISCOVERER_TREE: &str = "discoverer";

pub type State = Arc<Mutex<Vec<(ParserModel, ParserClient<Channel>)>>>;
pub type DB = Collection<ParserModel>;

pub struct LocalDiscoverer {
    state: State,

    /// Persists current state of parsers.
    db: DB,
}

impl LocalDiscoverer {
    pub fn new(state: Option<State>, db: DB) -> Self {
        Self {
            state: state.unwrap_or_default(),
            db,
        }
    }

    pub async fn new_from_db(db: DB) -> anyhow::Result<Self> {
        let state: State = Arc::default();

        let mut found = db.find(None, None).await?; // find all

        while let Some(parser) = found.try_next().await? {
            info!(
                id = parser._id.to_string(),
                host = &parser.host,
                port = &parser.port,
                parser_type = parser.parser_type.as_str_name(),
                "adding from database"
            );

            if let Err(err) = add_parser(parser.clone(), state.clone(), None).await {
                if err.downcast_ref::<tonic::transport::Error>().is_some() {
                    warn!(
                        id = parser._id.to_string(),
                        host = &parser.host,
                        port = &parser.port,
                        parser_type = parser.parser_type.as_str_name(),
                        "parser is no longer available, skipping"
                    );
                } else {
                    error!(
                        err = err.to_string(),
                        id = parser._id.to_string(),
                        host = &parser.host,
                        port = &parser.port,
                        parser_type = parser.parser_type.as_str_name(),
                        "could not add parser from db "
                    )
                }
            }
        }

        Ok(Self { db, state })
    }

    /// Functions spawn tokio task that takes care of updating LocalDiscoverer state by
    /// periodically call registered Parsers, if some one them won't respond, it's deleted from the state.
    pub async fn keep_parsers_in_sync(&self) {
        let state = self.state.clone();
        let db = self.db.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::new(0, 500)).await;
                let mut idx = 0;

                let mut locked = state.lock().await;
                while idx < locked.len() {
                    let (model, mut client) = locked[idx].clone();

                    if client
                        .health_check(communication::HealthCheckRequest {})
                        .await
                        .is_err()
                    {
                        locked.remove(idx);
                        match db.delete_one(doc! {"_id": model._id}, None).await {
                            Ok(result) => {
                                if result.deleted_count == 0 {
                                    warn!(
                                        id = model._id.to_string(),
                                        host = &model.host,
                                        port = &model.port,
                                        parser_type = model.parser_type.as_str_name(),
                                        "parser not found in db"
                                    )
                                } else {
                                    info!(
                                        id = model._id.to_string(),
                                        host = &model.host,
                                        port = &model.port,
                                        parser_type = model.parser_type.as_str_name(),
                                        "parser removed"
                                    )
                                }
                            }
                            Err(err) => error!(
                                err = err.to_string(),
                                id = model._id.to_string(),
                                host = &model.host,
                                port = &model.port,
                                parser_type = model.parser_type.as_str_name(),
                                "could not delete from db"
                            ),
                        }
                        continue;
                    }
                    idx += 1
                }
            }
        });
    }
}

/// Adds parsers to state and also to the db if provided.
async fn add_parser(parser: ParserModel, state: State, db: Option<DB>) -> anyhow::Result<()> {
    let client = communication::parser_client::ParserClient::connect(format!(
        "http://{}:{}",
        parser.host, parser.port
    ))
    .await?;

    if let Some(db) = db {
        db.insert_one(&parser, None)
            .await
            .context("could not insert to the db")?;
    }

    state.lock().await.push((parser, client));

    Ok(())
}

#[tonic::async_trait]
impl communication::discoverer_server::Discoverer for LocalDiscoverer {
    async fn register(
        &self,
        request: tonic::Request<communication::RegisterRequest>,
    ) -> GRPCResult<communication::RegisterResponse> {
        let data = request.into_inner();

        let parsed_parser_type = communication::ParserType::from_i32(data.p_type)
            .context("got invalid parser_type i32 value")
            .map_err(|e| {
                error!("error while parsing parser_type: {}", e);
                Status::internal("could not register client")
            })?;

        let id = bson::uuid::Uuid::parse_str(&data.id).map_err(|e| {
            error!("error while parsing id as uuid: {}", e);
            Status::invalid_argument("id must be uuid")
        })?;

        add_parser(
            ParserModel {
                _id: id,
                host: data.host.clone(),
                port: data.port,
                parser_type: parsed_parser_type,
            },
            self.state.clone(),
            Some(self.db.clone()),
        )
        .await
        .map_err(|e| {
            error!("error while creating client, {}", e);
            Status::internal("could not create client")
        })?;

        info!(
            host = &data.host,
            port = &data.port,
            parser_type = parsed_parser_type.as_str_name(),
            "parser registered"
        );

        Ok(Response::new(communication::RegisterResponse::default()))
    }

    async fn discover(
        &self,
        request: tonic::Request<communication::DiscoverRequest>,
    ) -> GRPCResult<communication::DiscoverResponse> {
        let data = request.into_inner();

        let parsed_parser_type = communication::ParserType::from_i32(data.p_type)
            .context("got invalid parser_type i32 value")
            .map_err(|e| {
                error!("error while parsing parser_type: {}", e);
                Status::internal("could not register client")
            })?;

        let urls: Vec<String> = self
            .state
            .lock()
            .await
            .iter()
            .filter(|(model, _)| model.parser_type == parsed_parser_type)
            .map(|(model, _)| format!("http://{}:{}", model.host, model.port))
            .collect();

        Ok(Response::new(communication::DiscoverResponse { urls }))
    }
}
