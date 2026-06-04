use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    body::Body,
    http::{Response, StatusCode, header},
};

#[derive(Clone, Debug, Default)]
pub struct AssetConfig {
    external_dist: Option<Arc<PathBuf>>,
}

impl AssetConfig {
    pub fn embedded() -> Self {
        Self {
            external_dist: None,
        }
    }

    pub fn with_external_dist(path: PathBuf) -> Self {
        Self {
            external_dist: Some(Arc::new(path)),
        }
    }

    pub async fn load(&self, request_path: &str) -> Result<Option<Asset>> {
        let relative = asset_relative_path(request_path);
        if let Some(root) = &self.external_dist {
            let path = root.join(&relative);
            if let Ok(bytes) = tokio::fs::read(&path).await {
                return Ok(Some(Asset::new(bytes, content_type(&relative))));
            }
        }
        embedded_asset(&relative)
    }
}

#[derive(Debug, Clone)]
pub struct Asset {
    bytes: Vec<u8>,
    content_type: &'static str,
}

impl Asset {
    fn new(bytes: Vec<u8>, content_type: &'static str) -> Self {
        Self {
            bytes,
            content_type,
        }
    }

    pub fn into_response(self) -> Response<Body> {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, self.content_type)
            .body(Body::from(self.bytes))
            .expect("asset response")
    }
}

fn asset_relative_path(request_path: &str) -> String {
    let path = request_path.trim_start_matches('/');
    if path.is_empty() || !path_has_extension(path) {
        return "index.html".to_string();
    }
    if let Some(index) = path.find("assets/") {
        return path[index..].to_string();
    }
    path.to_string()
}

fn path_has_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'))
}

fn embedded_asset(path: &str) -> Result<Option<Asset>> {
    let asset = match path {
        "index.html" => Asset::new(
            include_bytes!("../assets/index.html").to_vec(),
            "text/html; charset=utf-8",
        ),
        "assets/terminald.css" => Asset::new(
            include_bytes!("../assets/assets/terminald.css").to_vec(),
            "text/css; charset=utf-8",
        ),
        "assets/terminald.js" => Asset::new(
            include_bytes!("../assets/assets/terminald.js").to_vec(),
            "text/javascript; charset=utf-8",
        ),
        other if other.contains("..") => {
            return Err(anyhow::anyhow!("invalid asset path")).context("load embedded asset");
        }
        _ => return Ok(None),
    };
    Ok(Some(asset))
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        _ => "application/octet-stream",
    }
}
