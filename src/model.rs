use serde::Serialize;

use crate::communication;

#[derive(Debug, Clone, Copy, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct Message {
    pub parsed_type: ParsedType,
    pub content: String,
    pub tokens: Vec<String>,
}

impl From<communication::ParseContent> for Message {
    fn from(value: communication::ParseContent) -> Self {
        Self {
            parsed_type: value.parsed_type().into(),
            content: value.content,
            tokens: value.tokens,
        }
    }
}
