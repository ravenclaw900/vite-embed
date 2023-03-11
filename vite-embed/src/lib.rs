pub use vite_embed_macro::{vite_dev, vite_prod};

#[derive(Debug)]
pub struct EmbeddedFile {
    data: &'static [u8],
    asset_path: &'static str,
    compressed: bool,
}
