use crate::communication;
use crate::twoway;
use anyhow::Context;
use clap::{Parser, Subcommand};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Response;
use tonic::Status;
use tracing::error;

pub struct CliServer {
    tx: Arc<Mutex<twoway::Sender<communication::CliRequest, communication::CliResponse>>>,
}

impl CliServer {
    pub fn new(tx: twoway::Sender<communication::CliRequest, communication::CliResponse>) -> Self {
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

#[derive(Parser, Debug, Clone)]
pub struct CLI {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Searches indexes file by given statement.
    Search { statement: String },
}

/// Easy way of turning Command into communication::CliRequest.
impl From<Command> for communication::CliRequest {
    fn from(val: Command) -> Self {
        match val {
            Command::Search { statement } => communication::CliRequest {
                c_type: communication::CliType::Search.into(),
                content: statement,
            },
        }
    }
}
