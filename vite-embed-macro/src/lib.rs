use proc_macro::{Literal, Punct, TokenStream};

pub(crate) fn verify_sep(sep: &Punct, between: (usize, usize)) {
    if sep.as_char() != ',' {
        panic!(
            "Expected ',' between arguments {} and {}",
            between.0, between.1
        )
    }
}

pub(crate) fn unwrap_string_lit(lit: &Literal) -> String {
    let mut lit_string = lit.to_string();
    if !lit_string.starts_with('"') || !lit_string.ends_with('"') {
        panic!("Invalid string argument {}", lit_string)
    }

    // Remove quotes at beginning and end
    lit_string.remove(0);
    lit_string.pop();

    lit_string
}

#[cfg(feature = "dev")]
#[proc_macro]
pub fn generate_vite_dev(tokens: TokenStream) -> TokenStream {
    use dev::parse_tokens_html;
    use quote::quote;

    let macro_data = parse_tokens_html(tokens);

    let Ok(html) = std::fs::read_to_string(&macro_data.html_path) else {
        panic!("Couldn't read {:?} as string", macro_data.html_path)
    };

    let html = html.replace(
        "<!--vite-embed script injection-->",
        format!(
            r#"<script type="module" src="http://localhost:5173/@vite/client"></script>
<script type="module" src="http://localhost:5173/{}"></script>"#,
            macro_data.entry_point
        )
        .as_str(),
    );

    let html = html.as_bytes();

    let output = quote! {
        &[vite_embed::ViteData {
            path: "/",
            data: vite_embed::ViteFileData::UncompressedData(&[#(#html),*]),
            mime_type: "text/html"
        }]
    };

    TokenStream::from(output)
}

mod dev {
    use crate::{unwrap_string_lit, verify_sep};
    use proc_macro::{TokenStream, TokenTree};
    use std::path::PathBuf;

    pub(super) struct MacroDataDev {
        pub(super) html_path: PathBuf,
        pub(super) entry_point: String,
    }

    pub(super) fn parse_tokens_html(tokens: TokenStream) -> MacroDataDev {
        let token_vec: Vec<_> = tokens.into_iter().collect();

        match token_vec.as_slice() {
            [TokenTree::Literal(html_token), TokenTree::Punct(sep1), TokenTree::Literal(entry_point_token)] =>
            {
                verify_sep(sep1, (0, 1));
                let html_path = PathBuf::from(unwrap_string_lit(html_token).replace(
                    "$CARGO_MANIFEST_DIR",
                    &std::env::var("CARGO_MANIFEST_DIR").unwrap(),
                ));
                let entry_point = unwrap_string_lit(entry_point_token);
                MacroDataDev {
                    html_path,
                    entry_point,
                }
            }
            _ => panic!(
                r#"Expected format generate_vite_dev!("/path/to/index.html", "vite-entry-point.tsx")"#
            ),
        }
    }
}

#[cfg(feature = "prod")]
#[proc_macro]
pub fn generate_vite_prod(tokens: TokenStream) -> TokenStream {
    use prod::{compress, parse_tokens_prod};
    use quote::quote;
    use serde_json::Value;

    let manifest_path = parse_tokens_prod(tokens);

    let Ok(manifest_string) = std::fs::read_to_string(&manifest_path) else {
        panic!("Couldn't read {:?} as string", manifest_path);
    };

    let Ok(manifest_json) = serde_json::from_str::<Value>(&manifest_string) else {
        panic!("Couldn't parse {:?} as JSON", manifest_path);
    };

    let entry_point = manifest_json
        .as_object()
        .unwrap()
        .values()
        .find(|&f| {
            // If it exists, it has to be true
            f["isEntry"].is_boolean()
        })
        .unwrap_or_else(|| panic!("No entry point in manifest.json"));

    // Default file names that won't be part of manifest.json
    let mut file_names = vec!["index.html", "favicon.ico"];

    file_names.push(entry_point["file"].as_str().unwrap());

    if let Value::Array(arr) = &entry_point["css"] {
        for i in arr {
            file_names.push(i.as_str().unwrap());
        }
    }

    if let Value::Array(arr) = &entry_point["assets"] {
        for i in arr {
            file_names.push(i.as_str().unwrap());
        }
    }

    if let Value::Array(arr) = &entry_point["dynamicImports"] {
        for i in arr {
            file_names.push(manifest_json[i.as_str().unwrap()]["file"].as_str().unwrap());
        }
    }

    let mut mime_types = Vec::new();
    for &i in &file_names {
        mime_types.push(mime_guess::from_path(i).first_or_text_plain());
    }

    let mut file_datas = Vec::new();
    let mut compressed_datas = Vec::new();

    let parent_path = manifest_path.parent().unwrap();
    for (&name, mime) in file_names.iter().zip(mime_types.iter()) {
        let mut file_path = parent_path.to_path_buf();
        file_path.push(name);
        let Ok(file_data) = std::fs::read(&file_path) else {
            panic!("Couldn't read asset at {}", file_path.display());
        };
        match mime.type_() {
            mime_guess::mime::IMAGE | mime_guess::mime::VIDEO => {
                file_datas.push(file_data);
                compressed_datas.push(false);
            }
            _ => {
                let compressed = compress(&file_data);
                file_datas.push(compressed);
                compressed_datas.push(true);
            }
        }
    }

    // Prepend '/' to every item for HTTP paths
    let mut paths: Vec<_> = file_names
        .into_iter()
        .map(|x| {
            let mut x = x.to_string();
            x.insert(0, '/');
            x
        })
        .collect();

    // Replace index.html with just / as path
    // index.html should be at index 0 because that's how the Vec was created
    paths[0] = "/".to_string();

    let file_asts: Vec<_> = file_datas
        .into_iter()
        .zip(compressed_datas.into_iter())
        .map(|(data, compressed)| {
            if compressed {
                quote! {
                    vite_embed::ViteFileData::CompressedData(&[#(#data),*])
                }
            } else {
                quote! {
                    vite_embed::ViteFileData::UncompressedData(&[#(#data),*])
                }
            }
        })
        .collect();

    // Have to get as Strings because quote can't work with Mime structs
    let mime_types: Vec<_> = mime_types
        .into_iter()
        .map(|typ| typ.essence_str().to_string())
        .collect();

    quote! {
        &[#(vite_embed::ViteData {
            path: #paths,
            data: #file_asts,
            mime_type: #mime_types
        }),*]
    }
    .into()
}

#[cfg(feature = "prod")]
mod prod {
    use crate::unwrap_string_lit;
    use flate2::{write::GzEncoder, Compression};
    use proc_macro::{TokenStream, TokenTree};
    use std::io::Write;
    use std::path::PathBuf;

    pub(super) fn parse_tokens_prod(tokens: TokenStream) -> PathBuf {
        let token_vec: Vec<_> = tokens.into_iter().collect();

        match token_vec.as_slice() {
            [TokenTree::Literal(manifest_token)] => {
                PathBuf::from(unwrap_string_lit(manifest_token).replace(
                    "$CARGO_MANIFEST_DIR",
                    &std::env::var("CARGO_MANIFEST_DIR").unwrap(),
                ))
            }
            _ => {
                panic!(r#"Expected format generate_vite_prod!("/path/to/manifest.json")"#)
            }
        }
    }

    pub(super) fn compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(data).unwrap();
        let Ok(compressed) = encoder.finish() else {
            panic!("Couldn't GZIP data");
        };
        compressed
    }
}
