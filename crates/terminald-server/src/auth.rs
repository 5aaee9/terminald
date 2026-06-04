use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

const CHALLENGE: &str = r#"Basic realm="terminald""#;

#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    username: String,
    password: String,
}

impl Credential {
    pub fn new(value: &str) -> Option<Self> {
        let (username, password) = value.split_once(':')?;
        Some(Self {
            username: username.to_owned(),
            password: password.to_owned(),
        })
    }

    fn encoded(&self) -> String {
        STANDARD.encode(format!("{}:{}", self.username, self.password))
    }

    pub fn to_basic_pair(&self) -> String {
        format!("{}:{}", self.username, self.password)
    }
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default)]
pub struct AuthConfig {
    credential: Option<Credential>,
}

impl AuthConfig {
    pub fn disabled() -> Self {
        Self { credential: None }
    }

    pub fn basic(credential: Credential) -> Self {
        Self {
            credential: Some(credential),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.credential.is_some()
    }

    pub fn validate_headers(&self, headers: &HeaderMap) -> bool {
        let Some(credential) = &self.credential else {
            return true;
        };
        let Some(value) = headers.get(header::AUTHORIZATION) else {
            return false;
        };
        let Ok(value) = value.to_str() else {
            return false;
        };
        let Some(encoded) = value.strip_prefix("Basic ") else {
            return false;
        };
        constant_time_eq(encoded.as_bytes(), credential.encoded().as_bytes())
    }

    pub fn authorize_request(&self, request: &Request<Body>) -> Result<(), AuthRejection> {
        self.validate_headers(request.headers())
            .then_some(())
            .ok_or(AuthRejection)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AuthRejection;

impl IntoResponse for AuthRejection {
    fn into_response(self) -> Response {
        let mut response = StatusCode::UNAUTHORIZED.into_response();
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static(CHALLENGE),
        );
        response
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();
    for index in 0..max_len {
        let a = left.get(index).copied().unwrap_or_default();
        let b = right.get(index).copied().unwrap_or_default();
        diff |= usize::from(a ^ b);
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_debug_redacts_secret_parts() {
        let credential = Credential::new("user:pass").unwrap();
        let formatted = format!("{credential:?}");
        assert!(!formatted.contains("user"));
        assert!(!formatted.contains("pass"));
        assert!(!formatted.contains("Authorization"));
    }
}
