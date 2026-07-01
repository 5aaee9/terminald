use super::*;

use std::{path::Path, time::Instant};

use futures_util::{SinkExt, StreamExt};
use tempfile::TempDir;
use terminald_protocol::{ClientMessage, Resize, ServerMessage};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Duration, timeout},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as TungsteniteMessage, client::IntoClientRequest},
};

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

fn process_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn thread_count() -> usize {
    std::fs::read_dir("/proc/self/task").unwrap().count()
}

async fn wait_for_process_exit(pid: u32) -> bool {
    for _ in 0..100 {
        if !process_exists(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

async fn get_auth_check(base: &str) -> String {
    let address = base.strip_prefix("ws://").unwrap();
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(b"GET /auth/check HTTP/1.1\r\nHost: terminald\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
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

#[tokio::test(flavor = "current_thread")]
async fn websocket_close_terminates_pty_without_blocking_server() {
    let dir = TempDir::new().unwrap();
    let child_pid = dir.path().join("child-pid");
    let command = vec![
        "sh".into(),
        "-lc".into(),
        format!(
            "trap \"\" TERM; echo $$ > {child_pid}; printf ready; while true; do sleep 1; done",
            child_pid = child_pid.display(),
        ),
    ];
    let base = spawn_server_with_command(AuthConfig::disabled(), command).await;
    let (mut ws, _) = connect_async(format!("{base}/ws")).await.unwrap();
    assert!(next_output_text(&mut ws).await.contains("ready"));
    let pid = tokio::fs::read_to_string(&child_pid)
        .await
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    assert!(
        process_exists(pid),
        "PTY child exited before websocket closed"
    );

    let started = Instant::now();
    ws.send(TungsteniteMessage::Close(None)).await.unwrap();
    tokio::task::yield_now().await;
    let response = timeout(Duration::from_millis(100), get_auth_check(&base))
        .await
        .expect("server did not answer while websocket PTY was being cleaned up");
    assert!(response.starts_with("HTTP/1.1 204 No Content"));
    assert!(
        started.elapsed() < Duration::from_millis(150),
        "websocket close cleanup blocked the Tokio worker for {:?}",
        started.elapsed()
    );

    assert!(
        wait_for_process_exit(pid).await,
        "PTY child still alive after websocket closed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn websocket_supports_128_concurrent_ptys() {
    let dir = TempDir::new().unwrap();
    let child_pids = dir.path().join("child-pids");
    let base = spawn_server_with_command(
        AuthConfig::disabled(),
        vec![
            "sh".into(),
            "-lc".into(),
            format!(
                "echo $$ >> {child_pids}; printf ready; cat",
                child_pids = child_pids.display()
            ),
        ],
    )
    .await;
    let mut sockets = Vec::new();

    for _ in 0..128 {
        let (mut ws, _) = timeout(Duration::from_secs(5), connect_async(format!("{base}/ws")))
            .await
            .expect("timed out connecting websocket")
            .expect("websocket connection failed");
        timeout(Duration::from_secs(5), async {
            assert!(next_output_text(&mut ws).await.contains("ready"));
        })
        .await
        .expect("timed out waiting for PTY output");
        sockets.push(ws);
    }

    assert_eq!(sockets.len(), 128);
    assert!(
        thread_count() < 100,
        "128 idle PTYs should not require one blocking thread each"
    );

    let pids = tokio::fs::read_to_string(&child_pids)
        .await
        .unwrap()
        .lines()
        .map(|line| line.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(pids.len(), 128);

    for mut ws in sockets {
        ws.close(None).await.unwrap();
    }
    for pid in pids {
        assert!(
            wait_for_process_exit(pid).await,
            "PTY child {pid} still alive after websocket closed"
        );
    }
}
