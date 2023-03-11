use std::{fs, io::Write, path::PathBuf};

use flate2::{write::GzEncoder, Compression};
use proc_macro::{Literal, Punct, TokenStream, TokenTree};
use quote::quote;

struct MacroDataProd {
    manifest_path: PathBuf,
    entry_point: String,
    html_path: PathBuf,
}

struct MacroDataDev {
    html_path: PathBuf,
    entry_point: String,
}

fn parse_tokens_prod(tokens: TokenStream) -> MacroDataProd {
    let token_vec: Vec<_> = tokens.into_iter().collect();

    match token_vec.as_slice() {
        [TokenTree::Literal(manifest_token), TokenTree::Punct(sep1), TokenTree::Literal(entry_point_token), TokenTree::Punct(sep2), TokenTree::Literal(html_token)] =>
        {
            for i in [sep1, sep2].into_iter().enumerate() {
                verify_sep(i.1, (i.0, i.0 + 1))
            }
            let manifest_path = PathBuf::from(
                unwrap_string_lit(manifest_token)
                    .replace("$CARGO_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR")),
            );
            let html_path = PathBuf::from(
                unwrap_string_lit(html_token)
                    .replace("$CARGO_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR")),
            );
            let entry_point = unwrap_string_lit(entry_point_token);
            MacroDataProd {
                manifest_path,
                entry_point,
                html_path,
            }
        }
        _ => {
            panic!("Expected format 'string literal (path), string literal, string literal (path)'")
        }
    }
}

fn parse_tokens_dev(tokens: TokenStream) -> MacroDataDev {
    let token_vec: Vec<_> = tokens.into_iter().collect();

    match token_vec.as_slice() {
        [TokenTree::Literal(html_token), TokenTree::Punct(sep1), TokenTree::Literal(entry_point_token)] =>
        {
            verify_sep(sep1, (0, 1));
            let html_path = PathBuf::from(
                unwrap_string_lit(html_token)
                    .replace("$CARGO_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR")),
            );
            let entry_point = unwrap_string_lit(entry_point_token);
            MacroDataDev {
                html_path,
                entry_point,
            }
        }
        _ => panic!("Expected format 'string literal'"),
    }
}

fn verify_sep(sep: &Punct, between: (usize, usize)) {
    if sep.as_char() != ',' {
        panic!(
            "Expected ',' between arguments {} and {}",
            between.0, between.1
        )
    }
}

fn unwrap_string_lit(lit: &Literal) -> String {
    let mut lit_string = lit.to_string();
    if !lit_string.starts_with('"') || !lit_string.ends_with('"') {
        panic!("Invalid string argument {}", lit_string)
    }

    lit_string.remove(0);
    lit_string.pop();

    lit_string
}

#[proc_macro]
pub fn vite_dev(tokens: TokenStream) -> TokenStream {
    let macro_data = parse_tokens_dev(tokens);

    let Ok(html) = fs::read_to_string(&macro_data.html_path) else {
        panic!("Couldn't read {:?} as string", macro_data.html_path)
    };

    let html = html.replace(
        "<!--vite-embed injection point-->",
        format!(
            r#"<script type="module" src="http://localhost:5173/@vite/client"></script>
    <script type="module" src="http://localhost:5173/{}"></script>)"#,
            macro_data.entry_point
        )
        .as_str(),
    );

    let output = quote! {
        [vite_embed::EmbeddedFile { data: #html, asset_path: "index.html", compressed: false }]
    };

    TokenStream::from(output)
}

fn compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).unwrap();
    let Ok(compressed) = encoder.finish() else {
            panic!("Couldn't GZIP data");
        };
    compressed
}

#[proc_macro]
pub fn vite_prod(tokens: TokenStream) -> TokenStream {
    let macro_data = parse_tokens_prod(tokens);

    let Ok(html) = fs::read_to_string(&macro_data.html_path) else {
        panic!("Couldn't read {:?} as string", macro_data.html_path);
    };

    let Ok(manifest_string) = fs::read_to_string(&macro_data.manifest_path) else {
        panic!("Couldn't read {:?} as string", macro_data.manifest_path);
    };

    let Ok(manifest_json) = json::parse(&manifest_string) else {
        panic!("Couldn't parse {:?} as JSON", macro_data.manifest_path);
    };

    let mut file_names = Vec::new();

    file_names.push("favicon.png");

    let entry_point = &manifest_json[macro_data.entry_point];

    if !entry_point["isEntry"].as_bool().unwrap_or(false) {
        panic!("Wrong entry point");
    }

    file_names.push(entry_point["file"].as_str().unwrap());

    let mut inject = format!(
        r#"<script type="module" src="{}"></script>
        "#,
        file_names[0]
    );

    for i in entry_point["css"].members() {
        file_names.push(i.as_str().unwrap());
        inject.push_str(&format!(
            r#"<link rel="stylesheet" href="{}" />
            "#,
            i.as_str().unwrap()
        ));
    }

    for i in entry_point["assets"].members() {
        file_names.push(i.as_str().unwrap());
    }

    for i in entry_point["imports"].members() {
        file_names.push(manifest_json[i.as_str().unwrap()]["file"].as_str().unwrap());
    }

    let html = html.replace("<!--vite-embed injection point-->", &inject);
    let compressed_html = compress(html.as_bytes());

    let mut file_datas = Vec::new();
    let mut compressed_datas = Vec::new();
    let parent_path = macro_data.manifest_path.parent().unwrap();
    for i in &file_names {
        let mut file_path = parent_path.to_path_buf();
        file_path.push(i);
        let Ok(file_data) = fs::read(&file_path) else {
            panic!("Couldn't read asset at {}", file_path.display());
        };
        if file_path.extension().unwrap() == "png" {
            file_datas.push(file_data);
            compressed_datas.push(false);
        } else {
            let compressed = compress(&file_data);
            file_datas.push(compressed);
            compressed_datas.push(true);
        }
    }

    quote! {
        [vite_embed::EmbeddedFile {asset_path: "index.html", data: &[#(#compressed_html),*], compressed: true}, #(vite_embed::EmbeddedFile {asset_path: #file_names, data: &[#(#file_datas),*], compressed: #compressed_datas }),*]
    }.into()
}
