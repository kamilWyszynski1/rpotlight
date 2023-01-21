use rpot::{communication, parse};
use tonic::{async_trait, transport::Server, Response, Status};
use tracing::{error, info};

struct RustParser {}

#[async_trait]
impl communication::parser_server::Parser for RustParser {
    async fn parse(
        &self,
        request: tonic::Request<communication::ParseRequest>,
    ) -> Result<Response<communication::ParseResponse>, Status> {
        let file_path = request.into_inner().file_path;

        info!("received rust file to parse: {}", &file_path);

        Ok(Response::new(
            parse::parse_file_using_syn(file_path).map_err(|err| {
                error!("could not parse rust file {err}");
                Status::internal("could not parse rust file")
            })?,
        ))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let mut register =
        communication::register_client::RegisterClient::connect("http://[::1]:50051").await?;

    let addr = "[::1]:50052".parse()?;

    tokio::spawn(async move {
        Server::builder()
            .add_service(communication::parser_server::ParserServer::new(
                RustParser {},
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

    Ok(())
}
