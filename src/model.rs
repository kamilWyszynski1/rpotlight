use serde::{Deserialize, Serialize};

use crate::{communication, fts};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ParsedType {
    Function,
    Variable,
    Struct,
    None,
}

impl Default for ParsedType {
    fn default() -> Self {
        Self::None
    }
}

impl From<communication::ParsedType> for ParsedType {
    fn from(value: communication::ParsedType) -> Self {
        match value {
            communication::ParsedType::Function => Self::Function,
            communication::ParsedType::Variable => Self::Variable,
            communication::ParsedType::Struct => Self::Struct,
        }
    }
}

/// Duplicate of struct that is being generated from .proto in order to make it Serializable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Message {
    pub parsed_type: ParsedType,
    pub content: String,
    pub tokens: Vec<String>,
    pub parsed_content: String,
    pub file_line: u32,
}

impl From<communication::ParseContent> for Message {
    fn from(value: communication::ParseContent) -> Self {
        Self {
            parsed_type: value.parsed_type().into(),
            content: value.content,
            tokens: value.tokens,
            parsed_content: value.parsed_content,
            file_line: value.file_line,
        }
    }
}

/// ParseContent and file_path merged into one struct.
/// It's not sent via rpc like that not to duplicate data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParseContentWithPath {
    pub message: Message,
    pub file_path: String,
}

impl PartialEq for ParseContentWithPath {
    fn eq(&self, other: &Self) -> bool {
        self.message.content == other.message.content && self.file_path == other.file_path
    }
}

impl Eq for ParseContentWithPath {
    fn assert_receiver_is_total_eq(&self) {}
}

impl PartialOrd for ParseContentWithPath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.message.content.partial_cmp(&other.message.content)
    }
}

impl Ord for ParseContentWithPath {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.message.content.cmp(&other.message.content)
    }
}

impl fts::TokenProvider for ParseContentWithPath {
    fn get_tokens(&self) -> Vec<String> {
        self.message.tokens.clone()
    }

    fn id(&self) -> String {
        self.message.parsed_content.clone()
    }
}
