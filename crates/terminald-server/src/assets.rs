use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    body::Body,
    http::{Response, StatusCode, header},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets"]
struct EmbeddedAssets;

#[derive(Clone, Debug, Default)]
pub struct AssetConfig {
    external_dist: Option<Arc<PathBuf>>,
    use_embedded_dist: bool,
}

impl AssetConfig {
    pub fn embedded() -> Self {
        Self {
            external_dist: None,
            use_embedded_dist: true,
        }
    }

    pub fn with_external_dist(path: PathBuf) -> Self {
        Self {
            external_dist: Some(Arc::new(path)),
            use_embedded_dist: true,
        }
    }

    #[cfg(test)]
    pub fn embedded_fallback_only() -> Self {
        Self {
            external_dist: None,
            use_embedded_dist: false,
        }
    }

    pub async fn load(&self, request_path: &str) -> Result<Option<Asset>> {
        if has_parent_segment(request_path) {
            return Ok(None);
        }
        let relative = asset_relative_path(request_path);
        if let Some(root) = &self.external_dist {
            let path = root.join(&relative);
            if let Ok(bytes) = tokio::fs::read(&path).await {
                return Ok(Some(Asset::new(bytes, content_type(&relative))));
            }
        }
        embedded_asset(&relative, self.use_embedded_dist)
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

fn has_parent_segment(path: &str) -> bool {
    path.split('/').any(|segment| segment == "..")
}

fn embedded_asset(path: &str, use_dist: bool) -> Result<Option<Asset>> {
    let Some(candidate) = select_embedded_candidate(path, use_dist, |candidate| {
        EmbeddedAssets::get(candidate).is_some()
    }) else {
        return Ok(None);
    };
    let Some(file) = EmbeddedAssets::get(&candidate) else {
        return Err(anyhow::anyhow!("embedded asset disappeared")).context("load embedded asset");
    };
    Ok(Some(Asset::new(file.data.into_owned(), content_type(path))))
}

fn embedded_lookup_candidates(path: &str) -> [String; 2] {
    [format!("dist/{path}"), path.to_string()]
}

fn select_embedded_candidate(
    path: &str,
    use_dist: bool,
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    let candidates = embedded_lookup_candidates(path);
    candidates
        .into_iter()
        .filter(|candidate| use_dist || !candidate.starts_with("dist/"))
        .find(|candidate| exists(candidate))
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_traversal_before_external_or_embedded_lookup() {
        let config = AssetConfig::embedded();
        for path in [
            "/..",
            "/../index.html",
            "/assets/..",
            "/assets/../index.html",
            "/assets/../secret.txt",
        ] {
            assert!(config.load(path).await.unwrap().is_none());
        }

        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("dist");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("index.html"), "external index")
            .await
            .unwrap();
        tokio::fs::write(dir.join("secret.txt"), "external secret")
            .await
            .unwrap();
        tokio::fs::write(parent.path().join("outside.txt"), "outside secret")
            .await
            .unwrap();
        let config = AssetConfig::with_external_dist(dir);
        for path in [
            "/..",
            "/../index.html",
            "/../outside.txt",
            "/assets/..",
            "/assets/../index.html",
            "/assets/../secret.txt",
            "/assets/../outside.txt",
        ] {
            assert!(config.load(path).await.unwrap().is_none());
        }
    }

    #[test]
    fn selects_generated_dist_asset_before_fallback_asset() {
        let selected = select_embedded_candidate("index.html", true, |candidate| {
            matches!(candidate, "dist/index.html" | "index.html")
        })
        .unwrap();
        assert_eq!(selected, "dist/index.html");
    }

    #[test]
    fn can_select_fallback_without_generated_dist() {
        let selected = select_embedded_candidate("index.html", false, |candidate| {
            matches!(candidate, "dist/index.html" | "index.html")
        })
        .unwrap();
        assert_eq!(selected, "index.html");
    }
}
