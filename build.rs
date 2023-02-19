fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .type_attribute(
            "ParserType",
            "#[derive(serde::Deserialize, serde::Serialize)]",
        )
        .compile(&["proto/communication.proto"], &["proto"])?;
    Ok(())
}
