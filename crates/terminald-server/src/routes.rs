use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{FromRequestParts, State, WebSocketUpgrade},
    http::{Method, Request, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::any,
};

use crate::session::handle_socket;
use crate::{AssetConfig, AuthConfig, ServerConfig, auth::AuthRejection};

#[derive(Clone)]
pub struct AppState {
    pub auth: AuthConfig,
    pub assets: AssetConfig,
    #[allow(dead_code)]
    pub command: Arc<Vec<String>>,
}

pub fn app(config: ServerConfig) -> Router {
    let state = AppState {
        auth: config.auth,
        assets: config.assets,
        command: Arc::new(config.command),
    };
    Router::new()
        .route("/{*path}", any(dispatch))
        .fallback(any(dispatch))
        .with_state(state)
}

async fn dispatch(State(state): State<AppState>, request: Request<Body>) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();
    if path.ends_with("/auth/check") || path == "/auth/check" {
        return auth_check(&state.auth, &request);
    }
    if path.ends_with("/ws") || path == "/ws" {
        if state.auth.authorize_request(&request).is_err() {
            return AuthRejection.into_response();
        }
        let (mut parts, body) = request.into_parts();
        let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
            Ok(upgrade) => upgrade,
            Err(rejection) => return rejection.into_response(),
        };
        drop(body);
        let command = Arc::clone(&state.command);
        return upgrade
            .on_upgrade(move |socket| handle_socket(socket, (*command).clone()))
            .into_response();
    }
    if state.auth.authorize_request(&request).is_err() {
        return AuthRejection.into_response();
    }
    if is_extensionless_without_trailing_slash(&path, &method) {
        return Redirect::permanent(&format!("{path}/")).into_response();
    }
    match state.assets.load(&path).await {
        Ok(Some(asset)) => asset.into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            let message = format!("{error:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
        }
    }
}

fn auth_check(auth: &AuthConfig, request: &Request<Body>) -> Response {
    match auth.authorize_request(request) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(rejection) => rejection.into_response(),
    }
}

fn is_extensionless_without_trailing_slash(path: &str, method: &Method) -> bool {
    method == Method::GET
        && path != "/"
        && !path.ends_with('/')
        && !path.ends_with("/ws")
        && !path.ends_with("/auth/check")
        && !path_final_segment_has_extension(path)
}

fn path_final_segment_has_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use futures_util::{SinkExt, StreamExt};
    use http::header;
    use http_body_util::BodyExt;
    use tempfile::TempDir;
    use terminald_protocol::{ClientMessage, Resize, ServerMessage};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{Message as TungsteniteMessage, client::IntoClientRequest},
    };
    use tower::ServiceExt;

    use crate::Credential;

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

    async fn spawn_server(auth: AuthConfig) -> String {
        spawn_server_with_command(
            auth,
            vec!["sh".into(), "-lc".into(), "printf ready; cat".into()],
        )
        .await
    }

    async fn spawn_server_with_command(auth: AuthConfig, command: Vec<String>) -> String {
        let mut config = ServerConfig::new(0, command);
        config.auth = auth;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app(config)).await.unwrap();
        });
        format!("ws://{address}")
    }

    async fn next_output_text(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> String {
        loop {
            let Some(message) = ws.next().await else {
                panic!("websocket closed before output");
            };
            let TungsteniteMessage::Binary(frame) = message.unwrap() else {
                continue;
            };
            if let ServerMessage::Output(output) = ServerMessage::decode(&frame).unwrap() {
                return String::from_utf8_lossy(&output).into_owned();
            }
        }
    }

    async fn next_exit_code(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> i32 {
        loop {
            let Some(message) = ws.next().await else {
                panic!("websocket closed before exit frame");
            };
            let TungsteniteMessage::Binary(frame) = message.unwrap() else {
                continue;
            };
            if let ServerMessage::Exited(code) = ServerMessage::decode(&frame).unwrap() {
                return code;
            }
        }
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
        let router =
            app_with_fallback_assets(AuthConfig::basic(Credential::new("user:pass").unwrap()));
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

    #[tokio::test]
    async fn websocket_bridges_binary_input_and_resize() {
        let base = spawn_server(AuthConfig::disabled()).await;
        let (mut ws, _) = connect_async(format!("{base}/ws")).await.unwrap();
        assert!(next_output_text(&mut ws).await.contains("ready"));

        ws.send(TungsteniteMessage::Binary(
            ClientMessage::Input(b"hello\n".to_vec())
                .encode()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        assert!(next_output_text(&mut ws).await.contains("hello"));

        ws.send(TungsteniteMessage::Binary(
            ClientMessage::Resize(Resize {
                cols: 100,
                rows: 40,
            })
            .encode()
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn websocket_supports_prefixed_paths_and_auth() {
        let base = spawn_server(AuthConfig::disabled()).await;
        let (mut ws, _) = connect_async(format!("{base}/aaa/ws")).await.unwrap();
        assert!(next_output_text(&mut ws).await.contains("ready"));

        let base = spawn_server(AuthConfig::basic(Credential::new("user:pass").unwrap())).await;
        assert!(connect_async(format!("{base}/ws")).await.is_err());

        let mut request = format!("{base}/aaa/ws").into_client_request().unwrap();
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, auth_header());
        let (mut ws, _) = connect_async(request).await.unwrap();
        assert!(next_output_text(&mut ws).await.contains("ready"));
    }

    #[tokio::test]
    async fn websocket_text_input_reaches_pty() {
        let base = spawn_server(AuthConfig::disabled()).await;
        let (mut ws, _) = connect_async(format!("{base}/ws")).await.unwrap();
        assert!(next_output_text(&mut ws).await.contains("ready"));

        ws.send(TungsteniteMessage::Text("text hello\n".into()))
            .await
            .unwrap();
        assert!(next_output_text(&mut ws).await.contains("text hello"));
    }

    #[tokio::test]
    async fn websocket_invalid_resize_returns_error_frame() {
        let base = spawn_server(AuthConfig::disabled()).await;
        let (mut ws, _) = connect_async(format!("{base}/ws")).await.unwrap();
        assert!(next_output_text(&mut ws).await.contains("ready"));

        ws.send(TungsteniteMessage::Binary(vec![0, b'{'].into()))
            .await
            .unwrap();
        let message = ws.next().await.unwrap().unwrap();
        let TungsteniteMessage::Binary(frame) = message else {
            panic!("expected binary error frame");
        };
        let decoded = ServerMessage::decode(&frame).unwrap();
        assert!(
            matches!(decoded, ServerMessage::Error(error) if error.contains("invalid resize payload"))
        );
    }

    #[tokio::test]
    async fn websocket_sends_exit_frame_when_command_exits() {
        let base = spawn_server_with_command(
            AuthConfig::disabled(),
            vec!["sh".into(), "-lc".into(), "exit 7".into()],
        )
        .await;
        let (mut ws, _) = connect_async(format!("{base}/ws")).await.unwrap();

        assert_eq!(next_exit_code(&mut ws).await, 7);
    }
}
