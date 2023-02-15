use crate::{communication, GRPCResult};
use anyhow::Context;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, MutexGuard};
use tonic::{transport, Response, Status};
use tracing::{error, info, warn};

pub struct Parser(
    String,
    String,
    communication::parser_client::ParserClient<transport::Channel>,
);

pub const DISCOVERER_TREE: &str = "discoverer";

pub type State = Arc<Mutex<HashMap<communication::ParserType, Vec<Parser>>>>;
pub type Tree = Arc<Mutex<sled::Tree>>;

pub struct LocalDiscoverer {
    state: State,

    /// Persists current state of parsers.
    db: Tree,
}

impl LocalDiscoverer {
    pub fn new(state: Option<State>, db: Tree) -> Self {
        Self {
            state: state.unwrap_or_default(),
            db,
        }
    }

    pub async fn new_from_db(db: Tree) -> anyhow::Result<Self> {
        let state: State = Arc::default();

        let db_clone = db.clone();
        let locked = db_clone.lock().await;

        for msg in locked.into_iter() {
            let msg = msg?;

            let (host_port, p_type) = msg;

            let hp = String::from_utf8(host_port.to_vec())?;
            let pt = communication::ParserType::from_str_name(&String::from_utf8(p_type.to_vec())?)
                .context("could not build ParserType from pt")?;

            let host_port_spit = hp.split_once(':').context("could not split host_port")?;
            let host = host_port_spit.0.to_string();
            let port = host_port_spit.1.to_string();

            info!(
                host = &host,
                port = &port,
                p_type = pt.as_str_name(),
                "adding from database"
            );
            if let Err(err) =
                add_parser(host.clone(), port.clone(), pt, &locked, state.clone()).await
            {
                if err.downcast_ref::<tonic::transport::Error>().is_some() {
                    warn!(
                        host = &host,
                        port = &port,
                        p_type = pt.as_str_name(),
                        "parser is no longer available, skipping"
                    );
                } else {
                    error!(
                        err = err.to_string(),
                        host = &host,
                        port = &port,
                        p_type = pt.as_str_name(),
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
                for (_, parsers) in state.lock().await.iter_mut() {
                    let mut idx = 0;

                    while idx < parsers.len() {
                        if parsers[idx]
                            .2
                            .health_check(communication::HealthCheckRequest {})
                            .await
                            .is_err()
                        {
                            let (host, port) = (parsers[idx].0.clone(), parsers[idx].1.clone());

                            // has to be deleted
                            parsers.remove(idx);
                            match db.lock().await.remove(format!("{}:{}", &host, &port)) {
                                Ok(_) => info!(host = &host, port = &port, "parser removed"),
                                Err(err) => error!(
                                    err = err.to_string(),
                                    host = &host,
                                    port = &port,
                                    "could not delete from db"
                                ),
                            }

                            continue;
                        }
                        idx += 1
                    }
                }
            }
        });
    }
}

async fn add_parser<'a>(
    host: String,
    port: String,
    p_type: communication::ParserType,
    tree: &'a MutexGuard<'a, sled::Tree>,
    state: State,
) -> anyhow::Result<()> {
    state.lock().await.entry(p_type).or_default().push(Parser(
        host.clone(),
        port.clone(),
        communication::parser_client::ParserClient::connect(format!("http://{}:{}", host, port))
            .await?,
    ));

    tree.insert(format!("{}:{}", host, port), p_type.as_str_name())
        .context("could not insert to db")?;

    Ok(())
}

#[tonic::async_trait]
impl communication::discoverer_server::Discoverer for LocalDiscoverer {
    async fn register(
        &self,
        request: tonic::Request<communication::RegisterRequest>,
    ) -> GRPCResult<communication::RegisterResponse> {
        let data = request.into_inner();

        let parsed_p_type = communication::ParserType::from_i32(data.p_type)
            .context("got invalid p_type i32 value")
            .map_err(|e| {
                error!("error while parsing p_type: {}", e);
                Status::internal("could not register client")
            })?;

        add_parser(
            data.host.clone(),
            data.port.clone(),
            parsed_p_type,
            &self.db.clone().lock().await,
            self.state.clone(),
        )
        .await
        .map_err(|e| {
            error!("error while creating client, {}", e);
            Status::internal("could not create client")
        })?;

        info!(
            host = &data.host,
            port = &data.port,
            p_type = parsed_p_type.as_str_name(),
            "parser registered"
        );

        Ok(Response::new(communication::RegisterResponse::default()))
    }

    async fn discover(
        &self,
        request: tonic::Request<communication::DiscoverRequest>,
    ) -> GRPCResult<communication::DiscoverResponse> {
        let data = request.into_inner();

        let parsed_p_type = communication::ParserType::from_i32(data.p_type)
            .context("got invalid p_type i32 value")
            .map_err(|e| {
                error!("error while parsing p_type: {}", e);
                Status::internal("could not register client")
            })?;

        match self.state.lock().await.get(&parsed_p_type) {
            Some(parsers) => {
                return Ok(Response::new(communication::DiscoverResponse {
                    urls: parsers
                        .iter()
                        .map(|p| format!("http://{}:{}", p.0, p.1))
                        .collect(),
                }));
            }
            None => return Err(Status::not_found("no wanted parser registered")),
        }
    }
}
