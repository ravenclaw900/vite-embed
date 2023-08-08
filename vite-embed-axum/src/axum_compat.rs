use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
    response::IntoResponse,
    routing::get,
    Router,
};
use vite_embed::{ViteData, ViteEmbed, ViteFileData};

async fn route_static_frontend(State(asset): State<ViteData>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();

    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(asset.mime_type),
    );

    let data = match asset.data {
        ViteFileData::CompressedData(data) => {
            headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
            data
        }
        ViteFileData::UncompressedData(data) => data,
    };

    (headers, data)
}

#[cfg(feature = "dev")]
async fn vite_proxy(uri: Uri) -> Result<String, StatusCode> {
    let vite_resp = tokio::task::spawn_blocking(move || vite_embed::vite_proxy_dev(uri.path()))
        .await
        .expect("Failed to spawn blocking call to vite proxy");

    match vite_resp {
        Ok(body) => Ok(body),
        Err(vite_embed::RequestError::Status(code, _)) => Err(StatusCode::from_u16(code).unwrap()),
        _ => Err(StatusCode::BAD_GATEWAY),
    }
}

#[cfg(feature = "dev")]
pub fn vite_router(data: ViteEmbed) -> Router {
    Router::new()
        .route("/", get(route_static_frontend).with_state(data[0].clone()))
        .fallback(get(vite_proxy))
}

// Prefer using the dev version
#[cfg(all(feature = "prod", not(feature = "dev")))]
pub fn vite_router(data: ViteEmbed) -> Router {
    data.iter().fold(Router::new(), |router, asset| {
        router.route(
            asset.path,
            get(route_static_frontend).with_state(asset.clone()),
        )
    })
}
