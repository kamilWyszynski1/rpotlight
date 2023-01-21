use clap::Parser;
use rpot::cli;
use rpot::communication;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = communication::cli_client::CliClient::connect("http://[::1]:50053").await?;

    let cli = cli::CLI::parse();
    let response = client
        .command(communication::CliRequest::from(cli.command))
        .await?;
    println!("{}", response.into_inner().response);
    Ok(())
}
