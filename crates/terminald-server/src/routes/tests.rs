use super::*;

use axum::http::HeaderValue;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::header;
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

use crate::Credential;

mod websocket_tests;

fn app_with_auth(auth: AuthConfig) -> Router {
    let mut config = ServerConfig::new(7681, vec!["bash".to_string()]);
    config.auth = auth;
    app(config)
}

fn app_with_fallback_assets(auth: AuthConfig) -> Router {
    let mut config = ServerConfig::new(7681, vec!["bash".to_string()]);
    config.auth = auth;
    config.assets = AssetConfig::embedded_fallback_only();
    app(config)
}

fn auth_header() -> HeaderValue {
    HeaderValue::from_str(&format!("Basic {}", STANDARD.encode("user:pass"))).unwrap()
}

async fn get(router: Router, path: &str) -> Response {
    router
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn get_with_auth(router: Router, path: &str) -> Response {
    router
        .oneshot(
            Request::builder()
                .uri(path)
                .header(header::AUTHORIZATION, auth_header())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn body_text(response: Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn auth_check_matrix() {
    let disabled = app_with_auth(AuthConfig::disabled());
    for path in ["/auth/check", "/aaa/auth/check", "/example/bbb/auth/check"] {
        assert_eq!(
            get(disabled.clone(), path).await.status(),
            StatusCode::NO_CONTENT
        );
    }

    let enabled = app_with_auth(AuthConfig::basic(Credential::new("user:pass").unwrap()));
    for path in ["/auth/check", "/aaa/auth/check", "/example/bbb/auth/check"] {
        let response = get(enabled.clone(), path).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(header::WWW_AUTHENTICATE).unwrap(),
            r#"Basic realm="terminald""#
        );

        let response = get_with_auth(enabled.clone(), path).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}

#[tokio::test]
async fn redirects_extensionless_mounts() {
    let router = app_with_auth(AuthConfig::disabled());
    for (from, to) in [
        ("/aaa", "/aaa/"),
        ("/example/bbb", "/example/bbb/"),
        ("/custom", "/custom/"),
        ("/terminal/admin", "/terminal/admin/"),
    ] {
        let response = get(router.clone(), from).await;
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(response.headers().get(header::LOCATION).unwrap(), to);
    }

    for path in ["/", "/ws", "/auth/check", "/assets/terminald.css"] {
        let response = get(router.clone(), path).await;
        assert_ne!(response.status(), StatusCode::PERMANENT_REDIRECT);
    }
}

#[tokio::test]
async fn serves_embedded_index_for_app_routes() {
    let router = app_with_fallback_assets(AuthConfig::disabled());
    for path in [
        "/",
        "/aaa/",
        "/example/bbb/",
        "/foo/route/",
        "/example/bbb/client/path/",
    ] {
        let response = get(router.clone(), path).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("No frontend built"));
        assert!(!body.contains(r#"src="./assets/terminald.js""#));
    }
}

#[tokio::test]
async fn missing_embedded_static_assets_return_not_found_under_prefixes() {
    let router = app_with_fallback_assets(AuthConfig::disabled());
    for path in ["/assets/terminald.css", "/aaa/assets/terminald.css"] {
        let response = get(router.clone(), path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn protects_http_routes_when_auth_enabled() {
    let router = app_with_fallback_assets(AuthConfig::basic(Credential::new("user:pass").unwrap()));
    for path in ["/", "/aaa/", "/foo/route/", "/example/bbb/client/path/"] {
        assert_eq!(
            get(router.clone(), path).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let response = get_with_auth(router.clone(), path).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let css = "/assets/terminald.css";
    assert_eq!(
        get(router.clone(), css).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get_with_auth(router.clone(), css).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn serves_external_dist_before_embedded_assets() {
    let dir = TempDir::new().unwrap();
    let assets = dir.path().join("assets");
    tokio::fs::create_dir_all(&assets).await.unwrap();
    tokio::fs::write(dir.path().join("index.html"), "external dist app")
        .await
        .unwrap();
    tokio::fs::write(assets.join("terminald.css"), "external css")
        .await
        .unwrap();

    let mut config = ServerConfig::new(7681, vec!["bash".to_string()]);
    config.assets = AssetConfig::with_external_dist(dir.path().to_path_buf());
    let router = app(config);

    assert_eq!(
        body_text(get(router.clone(), "/").await).await,
        "external dist app"
    );
    assert_eq!(
        body_text(get(router, "/assets/terminald.css").await).await,
        "external css"
    );
}
