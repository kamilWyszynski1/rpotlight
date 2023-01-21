use crate::communication;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug, Clone)]
pub struct CLI {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Searches indexes file by given statement.
    Search { statement: String },
}

/// Easy way of turning Command into communication::CliRequest.
impl From<Command> for communication::CliRequest {
    fn from(val: Command) -> Self {
        match val {
            Command::Search { statement } => communication::CliRequest {
                c_type: communication::CliType::Search.into(),
                content: statement,
            },
        }
    }
}
