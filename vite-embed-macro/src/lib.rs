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

    lit_string.remove(0);
    lit_string.pop();

    lit_string
}

#[cfg(feature = "dev")]
#[proc_macro]
pub fn generate_vite_html_dev(tokens: TokenStream) -> TokenStream {
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
<script type="module" src="http://localhost:5173/{}"></script>)"#,
            macro_data.entry_point
        )
        .as_str(),
    );

    let output = quote! {
        fn vite_html_dev() -> &'static str {
            #html
        }
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
                r#"Expected format generate_vite_html_dev!("/path/to/index.html", "vite-entry-point.js")"#
            ),
        }
    }
}

#[cfg(feature = "prod")]
#[proc_macro]
pub fn generate_vite_prod(tokens: TokenStream) -> TokenStream {
    use prod::{compress, parse_tokens_prod};
    use quote::quote;

    let macro_data = parse_tokens_prod(tokens);

    let Ok(html) = std::fs::read_to_string(&macro_data.html_path) else {
        panic!("Couldn't read {:?} as string", macro_data.html_path);
    };

    let Ok(manifest_string) = std::fs::read_to_string(&macro_data.manifest_path) else {
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

    let html = html.replace(
        "<!--vite-embed script injection-->",
        &format!(r#"<script type="module" src="{}"></script>"#, file_names[0]),
    );

    let mut css_inject = String::new();

    for i in entry_point["css"].members() {
        file_names.push(i.as_str().unwrap());
        css_inject.push_str(&format!(
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

    let html = html.replace("<!--vite-embed css injection-->\n", &css_inject);
    let compressed_html = compress(html.as_bytes());

    let mut file_datas = Vec::new();
    let mut compressed_datas = Vec::new();
    let parent_path = macro_data.manifest_path.parent().unwrap();
    for i in &file_names {
        let mut file_path = parent_path.to_path_buf();
        file_path.push(i);
        let Ok(file_data) = std::fs::read(&file_path) else {
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

    // Append '/' to every item
    let file_names = file_names
        .into_iter()
        .map(|x| std::iter::once('/').chain(x.chars()).collect::<String>());

    quote! {
        fn vite_prod(path: &str) -> Option<(&'static [u8], bool)> {
            match path {
                "/index.html" => Some((&[#(#compressed_html),*], true)),
                #(#file_names => Some((&[#(#file_datas),*], #compressed_datas))),*,
                _ => None
            }
        }
    }
    .into()
}

#[cfg(feature = "prod")]
mod prod {
    use crate::{unwrap_string_lit, verify_sep};
    use flate2::{write::GzEncoder, Compression};
    use proc_macro::{TokenStream, TokenTree};
    use std::io::Write;
    use std::path::PathBuf;

    pub(super) struct MacroDataProd {
        pub(super) manifest_path: PathBuf,
        pub(super) entry_point: String,
        pub(super) html_path: PathBuf,
    }

    pub(super) fn parse_tokens_prod(tokens: TokenStream) -> MacroDataProd {
        let token_vec: Vec<_> = tokens.into_iter().collect();

        match token_vec.as_slice() {
            [TokenTree::Literal(manifest_token), TokenTree::Punct(sep1), TokenTree::Literal(entry_point_token), TokenTree::Punct(sep2), TokenTree::Literal(html_token)] =>
            {
                for i in [sep1, sep2].into_iter().enumerate() {
                    verify_sep(i.1, (i.0, i.0 + 1))
                }
                let manifest_path = PathBuf::from(unwrap_string_lit(manifest_token).replace(
                    "$CARGO_MANIFEST_DIR",
                    &std::env::var("CARGO_MANIFEST_DIR").unwrap(),
                ));
                let html_path = PathBuf::from(unwrap_string_lit(html_token).replace(
                    "$CARGO_MANIFEST_DIR",
                    &std::env::var("CARGO_MANIFEST_DIR").unwrap(),
                ));
                let entry_point = unwrap_string_lit(entry_point_token);
                MacroDataProd {
                    manifest_path,
                    entry_point,
                    html_path,
                }
            }
            _ => {
                panic!(
                    r#"Expected format generate_vite_prod!("/path/to/manifest.json", "vite-entry-point", "/path/to/index.html")"#
                )
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
