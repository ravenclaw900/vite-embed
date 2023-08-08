#[cfg(feature = "dev")]
pub use vite_embed_macro::generate_vite_dev;

#[cfg(feature = "prod")]
pub use vite_embed_macro::generate_vite_prod;

#[derive(Debug, Clone)]
pub enum ViteFileData {
    CompressedData(&'static [u8]),
    UncompressedData(&'static [u8]),
}

#[derive(Debug, Clone)]
pub struct ViteData {
    pub path: &'static str,
    pub data: ViteFileData,
    pub mime_type: &'static str,
}

pub type ViteEmbed = &'static [ViteData];

#[cfg(feature = "dev")]
pub use ureq::Error as RequestError;

#[cfg(feature = "dev")]
pub fn vite_proxy_dev(path: &str) -> Result<String, RequestError> {
    Ok(ureq::get(&format!("http://localhost:5173{}", path))
        .call()?
        .into_string()?)
}
