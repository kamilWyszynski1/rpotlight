pub mod cli;
pub mod fts;
pub mod model;
pub mod parse;
pub mod read;
pub mod twoway;

pub mod communication {
    tonic::include_proto!("communication");
}
