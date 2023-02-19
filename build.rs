fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .type_attribute(
            "ParserType",
            "#[derive(serde::Deserialize, serde::Serialize)]",
        )
        .type_attribute(
            "ParseResponse",
            "#[derive(serde::Deserialize, serde::Serialize)]",
        )
        .type_attribute(
            "ParseContent",
            "#[derive(serde::Deserialize, serde::Serialize)]",
        )
        .type_attribute(
            "ParsedType",
            "#[derive(serde::Deserialize, serde::Serialize)]",
        )
        .compile(&["proto/communication.proto"], &["proto"])?;
    Ok(())
}
