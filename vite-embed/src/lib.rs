pub use vite_embed_macro::{vite_dev, vite_prod};

#[derive(Debug)]
pub struct EmbeddedFile {
    pub data: &'static [u8],
    pub asset_path: &'static str,
    pub compressed: bool,
}
