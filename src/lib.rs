pub mod parse;
pub mod read;

pub mod communication {
    tonic::include_proto!("communication");
}
