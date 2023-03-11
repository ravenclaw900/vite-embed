#[derive(Debug)]
pub struct EmbeddedFile {
    data: &'static [u8],
    asset_path: &'static str,
    compressed: bool,
}
