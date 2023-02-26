use crate::communication::{ParseContent, ParseResponse, ParsedType};
use crate::file_checksum;
use crate::fts::{self, tokenize};
use lazy_static::lazy_static;
use regex::Regex;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Lines, Read};
use std::path::Path;
use tracing::error;

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

pub enum ParserVariant {
    Regex,
    Syn,
}

impl ParserVariant {
    pub fn parse(&self, file_path: String) -> anyhow::Result<ParseResponse> {
        match self {
            ParserVariant::Regex => regex_parse(file_path),
            ParserVariant::Syn => syn_parse(file_path),
        }
    }
}

fn syn_parse(file_path: String) -> anyhow::Result<ParseResponse> {
    let mut file = File::open(&file_path)?;

    let mut src = String::new();
    file.read_to_string(&mut src)?;

    let syntax = syn::parse_file(&src)?;

    let mut response = ParseResponse {
        checksum: file_checksum(&file_path)?,
        file_path,
        content: vec![],
    };
    for item in syntax.items {
        if let syn::Item::Impl(im) = item {
            for impl_item in im.items {
                if let syn::ImplItem::Method(method) = impl_item {
                    let method_name = method.sig.ident.to_string();

                    response.content.push(ParseContent {
                        parsed_type: ParsedType::Function.into(),
                        content: method_name.clone(),
                        tokens: fts::tokenize(method_name),
                        ..Default::default()
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
                content: content.clone(),
                tokens: fts::tokenize(content),
                ..Default::default()
            })
        }
    }
    Ok(response)
}
fn regex_parse(file_path: String) -> anyhow::Result<ParseResponse> {
    lazy_static! {
        static ref FN: Regex =
            Regex::new(r"(?m)(.*fn\s+(async\s+)?(\w+)\s*(<.*>)?\s*\(.*)").unwrap();
        static ref STRUCT: Regex = Regex::new(r"(?m)(.*(trait|enum|struct)\s+(\w+).*)").unwrap();
    }

    let lines = read_lines(&file_path)?;

    let mut response = ParseResponse {
        checksum: file_checksum(&file_path)?,
        file_path,
        ..Default::default()
    };
    for (i, line) in lines.into_iter().enumerate() {
        match line {
            Ok(line) => {
                // search for function
                match FN.captures(line.as_str()) {
                    Some(cap) => {
                        let content = cap.get(0).unwrap().as_str().trim().to_string();
                        let name = cap.get(3).unwrap().as_str().trim();

                        response.content.push(ParseContent {
                            parsed_type: ParsedType::Function.into(),
                            content,
                            tokens: tokenize(name),
                            file_line: i as u32 + 1,
                            parsed_content: name.to_string(),
                        })
                    }
                    // try with struct
                    None => match STRUCT.captures(line.as_str()) {
                        Some(cap) => {
                            let content = cap.get(0).unwrap().as_str().trim().to_string();
                            let name = cap.get(3).unwrap().as_str().trim();

                            response.content.push(ParseContent {
                                parsed_type: ParsedType::Struct.into(),
                                content,
                                tokens: tokenize(name),
                                file_line: i as u32 + 1,
                                parsed_content: name.to_string(),
                            })
                        }
                        None => continue,
                    },
                }
            }
            Err(err) => {
                error!(err = err.to_string(), "error occured during reading lines");
                break;
            }
        }
    }

    Ok(response)
}

fn read_lines<P>(path: P) -> io::Result<Lines<BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(path)?;
    Ok(BufReader::new(file).lines())
}
