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
