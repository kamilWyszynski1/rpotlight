pub mod cli;
pub mod db;
pub mod discoverer;
pub mod fts;
pub mod migrations;
pub mod model;
pub mod parse;
pub mod read;
pub mod registry;
pub mod twoway;
pub mod watcher;

pub mod communication {
    tonic::include_proto!("communication");
}

pub type GRPCResult<T> = Result<tonic::Response<T>, tonic::Status>;
