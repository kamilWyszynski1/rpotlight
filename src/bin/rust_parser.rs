use rpot::{
    communication::{self},
    parse::ParserVariant,
    GRPCResult,
};
use tonic::{async_trait, transport::Server, Response, Status};
use tracing::{error, info};

struct RustParser {
    parser: ParserVariant,
}

#[async_trait]
impl communication::parser_server::Parser for RustParser {
    async fn parse(
        &self,
        request: tonic::Request<communication::ParseRequest>,
    ) -> GRPCResult<communication::ParseResponse> {
        let file_path = request.into_inner().file_path;

        info!(path = file_path, "received rust file to parse");

        let parsed = self.parser.parse(file_path.clone()).map_err(|err| {
            error!("could not parse rust file {err}");
            Status::internal("could not parse rust file")
        });
        info!(path = file_path, success = parsed.is_ok(), "file parsed");

        Ok(Response::new(parsed?))
    }

    async fn health_check(
        &self,
        _: tonic::Request<communication::HealthCheckRequest>,
    ) -> GRPCResult<communication::HealthCheckResponse> {
        Ok(Response::new(communication::HealthCheckResponse {}))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let mut register =
        communication::discoverer_client::DiscovererClient::connect("http://[::1]:50059").await?;

    let addr = "[::1]:50052".parse()?;

    let server_thread = tokio::spawn(async move {
        Server::builder()
            .add_service(communication::parser_server::ParserServer::new(
                RustParser {
                    parser: ParserVariant::Regex,
                },
            ))
            .serve(addr)
            .await
            .unwrap();
    });

    register
        .register(communication::RegisterRequest {
            host: "localhost".to_string(),
            port: "50052".to_string(),
            p_type: communication::ParserType::Rust.into(),
        })
        .await
        .unwrap();

    server_thread.await?;
    Ok(())
}
