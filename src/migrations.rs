use crate::{communication::ParseResponse, registry::ParsedModel};
use bson::doc;
use mongodb::{options::IndexOptions, Database, IndexModel};

pub async fn parsed_migrations(client: &Database) {
    let options = IndexOptions::builder().unique(true).build();
    let model = IndexModel::builder()
        .keys(doc! {"file_path": 1})
        .options(options)
        .build();
    client
        .collection::<ParsedModel>("parsed")
        .create_index(model, None)
        .await
        .expect("error creating index!");
}

pub async fn content_migrations(client: &Database) {
    let options = IndexOptions::builder().unique(true).build();
    let model = IndexModel::builder()
        .keys(doc! {"file_path": 1})
        .options(options)
        .build();
    client
        .collection::<ParseResponse>("content")
        .create_index(model, None)
        .await
        .expect("error creating index!");
}
