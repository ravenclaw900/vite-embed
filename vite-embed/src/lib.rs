#[cfg(feature = "dev")]
pub use vite_embed_macro::generate_vite_html_dev;

#[cfg(feature = "prod")]
pub use vite_embed_macro::generate_vite_prod;

#[cfg(feature = "dev")]
pub use ureq::Error as RequestError;

#[cfg(feature = "dev")]
pub fn vite_proxy_dev(path: &str) -> Result<String, RequestError> {
    Ok(ureq::get(&format!("http://localhost:5173{}", path))
        .call()?
        .into_string()?)
}
