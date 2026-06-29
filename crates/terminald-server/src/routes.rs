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
mod tests;
