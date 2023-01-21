use clap::Parser;
use rpot::cli;
use rpot::communication;
use rpot::model::ParseContentWithPath;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = communication::cli_client::CliClient::connect("http://[::1]:50053").await?;

    let cli = cli::CLI::parse();
    let response = client
        .command(communication::CliRequest::from(cli.command))
        .await?;

    let a: Vec<ParseContentWithPath> =
        serde_json::from_str(response.into_inner().response.as_str())?;

    println!("searches:");
    for content in a {
        println!(
            "\t {} in {}:{}",
            content.message.parsed_content, content.file_path, content.message.file_line
        )
    }

    Ok(())
}
