#![cfg(unix)]

use std::{os::unix::fs::FileTypeExt, path::Path, time::Duration};

use axum::http::{HeaderValue, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use tempfile::TempDir;
use terminald_protocol::{ClientMessage, Resize, ServerMessage};
use terminald_server::{AuthConfig, Credential, ServerConfig, serve};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UnixListener, UnixStream},
    task::JoinHandle,
    time::{sleep, timeout},
};
use tokio_tungstenite::{
    WebSocketStream, client_async,
    tungstenite::{Error, Message, client::IntoClientRequest},
};

struct RunningServer(JoinHandle<anyhow::Result<()>>);
impl Drop for RunningServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn config(path: &Path) -> ServerConfig {
    let mut config = ServerConfig::new(
        0,
        vec!["sh".into(), "-c".into(), "printf ready; cat".into()],
    );
    config.unix_socket = Some(path.to_owned());
    config.auth = AuthConfig::basic(Credential::new("user:pass").unwrap());
    config
}

async fn start(config: ServerConfig) -> RunningServer {
    let path = config.unix_socket.clone().unwrap();
    let server = RunningServer(tokio::spawn(serve(config)));
    timeout(Duration::from_secs(5), async {
        loop {
            if UnixStream::connect(&path).await.is_ok() {
                break;
            }
            assert!(!server.0.is_finished(), "server exited before readiness");
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Unix socket did not become ready");
    server
}

async fn http(path: &Path, credential: Option<&str>) -> String {
    let mut stream = UnixStream::connect(path).await.unwrap();
    let auth = credential
        .map(|c| format!("Authorization: Basic {}\r\n", STANDARD.encode(c)))
        .unwrap_or_default();
    stream.write_all(format!("GET /prefix/auth/check HTTP/1.1\r\nHost: upstream.internal\r\n{auth}Connection: close\r\n\r\n").as_bytes()).await.unwrap();
    let mut response = String::new();
    timeout(Duration::from_secs(5), stream.read_to_string(&mut response))
        .await
        .unwrap()
        .unwrap();
    response
}

async fn output(ws: &mut WebSocketStream<UnixStream>) -> String {
    timeout(Duration::from_secs(5), async {
        loop {
            let message = ws.next().await.expect("socket closed").unwrap();
            if let Message::Binary(frame) = message
                && let ServerMessage::Output(bytes) = ServerMessage::decode(&frame).unwrap()
            {
                return String::from_utf8_lossy(&bytes).into_owned();
            }
        }
    })
    .await
    .expect("PTY output timed out")
}

#[tokio::test]
async fn unix_socket_serves_http_auth_and_prefixed_websocket_pty_without_tcp() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("terminal.sock");
    // A Unix listener must work even if the config's TCP port is unavailable.
    let tcp = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let mut cfg = config(&path);
    cfg.port = tcp.local_addr().unwrap().port();
    let _server = start(cfg).await;
    assert!(std::fs::metadata(&path).unwrap().file_type().is_socket());
    for (credential, status) in [
        (None, 401),
        (Some("user:wrong"), 401),
        (Some("user:pass"), 204),
    ] {
        assert!(
            http(&path, credential)
                .await
                .starts_with(&format!("HTTP/1.1 {status}"))
        );
    }
    for credential in [None, Some("user:wrong"), Some("user:pass")] {
        let mut request = "ws://upstream.internal/mounted/ws"
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static("https://console.example"),
        );
        if let Some(credential) = credential {
            request.headers_mut().insert(
                header::AUTHORIZATION,
                format!("Basic {}", STANDARD.encode(credential))
                    .parse()
                    .unwrap(),
            );
        }
        let stream = UnixStream::connect(&path).await.unwrap();
        let result = timeout(Duration::from_secs(5), client_async(request, stream))
            .await
            .unwrap();
        if credential != Some("user:pass") {
            let Error::Http(response) = result.unwrap_err() else {
                panic!("expected HTTP auth rejection");
            };
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            continue;
        }
        let (mut ws, response) = result.unwrap();
        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        assert!(output(&mut ws).await.contains("ready"));
        ws.send(Message::Binary(
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
        ws.send(Message::Binary(
            ClientMessage::Input(b"socket-input\n".to_vec())
                .encode()
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        let mut text = String::new();
        timeout(Duration::from_secs(5), async {
            while !text.contains("socket-input") {
                text.push_str(&output(&mut ws).await);
            }
        })
        .await
        .unwrap();
        ws.close(None).await.unwrap();
    }
}

async fn bind_error(path: &Path) {
    let error = timeout(Duration::from_secs(2), serve(config(path)))
        .await
        .expect("must not replace an existing socket or create a missing parent")
        .unwrap_err();
    assert!(format!("{error:#}").contains("bind terminald Unix socket"));
    assert!(error.chain().count() > 1, "bind error lost its IO cause");
}

#[tokio::test]
async fn unix_socket_refuses_existing_files_sockets_and_symlinks() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("regular-file");
    std::fs::write(&file, "keep me").unwrap();
    bind_error(&file).await;
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep me");

    let symlink = dir.path().join("symlink");
    std::os::unix::fs::symlink(&file, &symlink).unwrap();
    bind_error(&symlink).await;
    assert!(symlink.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep me");

    let live = dir.path().join("live.sock");
    let listener = UnixListener::bind(&live).unwrap();
    bind_error(&live).await;
    let _connection = UnixStream::connect(&live).await.unwrap();
    drop(listener);
    bind_error(&live).await; // Stale sockets are not silently unlinked either.
    assert!(live.metadata().unwrap().file_type().is_socket());
    let absent = dir.path().join("absent");
    bind_error(&absent.join("terminal.sock")).await;
    assert!(!absent.exists());
}
