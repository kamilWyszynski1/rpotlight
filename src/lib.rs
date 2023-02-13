pub mod cli;
pub mod discoverer;
pub mod fts;
pub mod model;
pub mod parse;
pub mod read;
pub mod registry;
pub mod twoway;
pub mod watcher;

pub mod communication {
    tonic::include_proto!("communication");
}

/// Common types
use std::sync::Arc;
use tokio::sync::Mutex;

pub type GRPCResult<T> = Result<tonic::Response<T>, tonic::Status>;

/// Type wrap for database.
pub type DB = Arc<Mutex<sled::Db>>;
