use crate::communication::{ParseDetails, ParseResponse};
use anyhow::Ok;
use std::fs::File;
use std::io::Read;

#[derive(Debug)]
enum ParseInfo {
    Function(String),
    Variable(String),
    Struct(String),
}

pub fn parse_file_using_syn(file_path: String) -> anyhow::Result<ParseResponse> {
    let mut file = File::open(&file_path)?;

    let mut src = String::new();
    file.read_to_string(&mut src)?;

    let syntax = syn::parse_file(&src)?;

    let mut response = ParseResponse {
        file_path,
        ..Default::default()
    };
    for item in syntax.items {
        if let syn::Item::Impl(im) = item {
            for impl_item in im.items {
                if let syn::ImplItem::Method(method) = impl_item {
                    response.functions.insert(
                        method.sig.ident.to_string(),
                        ParseDetails {
                            line_number: 0,
                            whole_line: "".into(),
                        },
                    );
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

            match info {
                ParseInfo::Function(f) => response.functions.insert(
                    f,
                    ParseDetails {
                        line_number: 0,
                        whole_line: "".into(),
                    },
                ),
                ParseInfo::Variable(v) => response.variables.insert(
                    v,
                    ParseDetails {
                        line_number: 0,
                        whole_line: "".into(),
                    },
                ),
                ParseInfo::Struct(s) => response.structs.insert(
                    s,
                    ParseDetails {
                        line_number: 0,
                        whole_line: "".into(),
                    },
                ),
            };
        }
    }
    Ok(response)
}
