use crate::{communication, GRPCResult};
use anyhow::Context;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use tonic::{transport, Response, Status};
use tracing::{error, info};

struct Parser(
    String,
    String,
    communication::parser_client::ParserClient<transport::Channel>,
);

#[derive(Default)]
pub struct LocalDiscoverer {
    state: Arc<Mutex<HashMap<communication::ParserType, Vec<Parser>>>>,
}

impl LocalDiscoverer {
    /// Functions spawn tokio task that takes care of updating LocalDiscoverer state by
    /// periodically call registered Parsers, if some one them won't respond, it's deleted from the state.
    pub async fn keep_parsers_in_sync(&self) {
        let state = self.state.clone();
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
                            info!(
                                host = parsers[idx].0,
                                port = parsers[idx].1,
                                "deleted from the state"
                            );
                            // has to be deleted
                            parsers.remove(idx);
                            continue;
                        }
                        idx += 1
                    }
                }
            }
        });
    }
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
        self.state
            .lock()
            .await
            .entry(parsed_p_type)
            .or_default()
            .push(Parser(
                data.host.clone(),
                data.port.clone(),
                communication::parser_client::ParserClient::connect(format!(
                    "http://{}:{}",
                    data.host, data.port
                ))
                .await
                .map_err(|e| {
                    error!("error while creating client, {}", e);
                    Status::internal("could not create client")
                })?,
            ));

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
                    urls: parsers.iter().map(|p| format!("{}:{}", p.0, p.1)).collect(),
                }));
            }
            None => return Err(Status::not_found("no wanted parser registered")),
        }
    }
}
