use std::{collections::HashMap, sync::Arc};

use crate::communication;
use anyhow::Context;
use tokio::sync::Mutex;
use tonic::{Response, Status};
use tracing::{error, info};

struct Parser(String, String);

// TODO: implement health checks and parsers map updates
struct LocalDiscoverer {
    parsers: Arc<Mutex<HashMap<communication::ParserType, Vec<Parser>>>>,
}

#[tonic::async_trait]
impl communication::discoverer_server::Discoverer for LocalDiscoverer {
    async fn register(
        &self,
        request: tonic::Request<communication::RegisterRequest>,
    ) -> Result<Response<communication::RegisterResponse>, Status> {
        let data = request.into_inner();

        let parsed_p_type = communication::ParserType::from_i32(data.p_type)
            .context("got invalid p_type i32 value")
            .map_err(|e| {
                error!("error while parsing p_type: {}", e);
                Status::internal("could not register client")
            })?;
        self.parsers
            .lock()
            .await
            .entry(parsed_p_type)
            .or_default()
            .push(Parser(data.host, data.port));

        Ok(Response::new(communication::RegisterResponse::default()))
    }

    async fn discover(
        &self,
        request: tonic::Request<communication::DiscoverRequest>,
    ) -> Result<Response<communication::DiscoverResponse>, Status> {
        let data = request.into_inner();

        let parsed_p_type = communication::ParserType::from_i32(data.p_type)
            .context("got invalid p_type i32 value")
            .map_err(|e| {
                error!("error while parsing p_type: {}", e);
                Status::internal("could not register client")
            })?;

        match self.parsers.lock().await.get(&parsed_p_type) {
            Some(parsers) => {
                return Ok(Response::new(communication::DiscoverResponse {
                    urls: parsers.iter().map(|p| format!("{}:{}", p.0, p.1)).collect(),
                }));
            }
            None => return Err(Status::not_found("no wanted parser registered")),
        }
    }
}
