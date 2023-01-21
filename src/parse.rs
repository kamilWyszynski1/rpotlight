use crate::communication::{ParseContent, ParseResponse, ParsedType};
use anyhow::Ok;
use std::fs::File;
use std::io::Read;

#[derive(Debug)]
enum ParseInfo {
    Function(String),
    Variable(String),
    Struct(String),
}

impl ParseInfo {
    fn rpc_type_and_content(self) -> (i32, String) {
        match self {
            ParseInfo::Function(value) => (ParsedType::Function.into(), value),
            ParseInfo::Variable(value) => (ParsedType::Variable.into(), value),
            ParseInfo::Struct(value) => (ParsedType::Struct.into(), value),
        }
    }
}

pub fn parse_file_using_syn(file_path: String) -> anyhow::Result<ParseResponse> {
    let mut file = File::open(&file_path)?;

    let mut src = String::new();
    file.read_to_string(&mut src)?;

    let syntax = syn::parse_file(&src)?;

    let mut response = ParseResponse {
        file_path,
        content: vec![],
    };
    for item in syntax.items {
        if let syn::Item::Impl(im) = item {
            for impl_item in im.items {
                if let syn::ImplItem::Method(method) = impl_item {
                    response.content.push(ParseContent {
                        parsed_type: ParsedType::Function.into(),
                        content: method.sig.ident.to_string(),
                        tokens: vec![],
                    });
                }
            }
        } else {
            let info = match &item {
                syn::Item::Const(c) => ParseInfo::Variable(c.ident.to_string()),
                syn::Item::Enum(e) => ParseInfo::Struct(e.ident.to_string()),
                syn::Item::Fn(f) => ParseInfo::Function(f.sig.ident.to_string()),
                syn::Item::Static(s) => ParseInfo::Variable(s.ident.to_string()),
                syn::Item::Struct(s) => ParseInfo::Struct(s.ident.to_string()),
                syn::Item::Trait(t) => ParseInfo::Struct(t.ident.to_string()),
                syn::Item::TraitAlias(t) => ParseInfo::Struct(t.ident.to_string()),
                syn::Item::Type(t) => ParseInfo::Struct(t.ident.to_string()),
                _ => continue,
            };
            let (parsed_type, content) = info.rpc_type_and_content();
            response.content.push(ParseContent {
                parsed_type,
                content,
                tokens: vec![],
            })
        }
    }
    Ok(response)
}
