use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use http::header;
use terminald_protocol::{ClientMessage, ServerMessage};
use terminald_server::Credential;
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::mpsc,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use url::Url;

use crate::terminal::{RawTerminalGuard, TerminalEvents};

#[derive(Debug)]
pub struct ClientConfig {
    pub connect: String,
    pub credential: Option<Credential>,
}

pub async fn run(config: ClientConfig) -> Result<()> {
    let _guard = RawTerminalGuard::enter()?;
    let ws_url = resolve_ws_url(&config.connect)?;
    let mut request = ws_url.as_str().into_client_request()?;
    if let Some(header) = basic_auth_header(&config.credential) {
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, header.parse()?);
    }

    let (socket, _) = connect_async(request)
        .await
        .context("connect to terminald server")?;
    let (mut writer, mut reader) = socket.split();
    let (resize_tx, mut resize_rx) = mpsc::channel(8);
    let events = TerminalEvents::new(resize_tx)?;

    if let Some(size) = events.current_size() {
        writer
            .send(Message::Binary(
                ClientMessage::Resize(size).encode()?.into(),
            ))
            .await?;
    }

    let writer_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0_u8; 4096];
        loop {
            tokio::select! {
                read = stdin.read(&mut buf) => {
                    let read = read?;
                    if read == 0 {
                        break;
                    }
                    writer
                        .send(Message::Binary(ClientMessage::Input(buf[..read].to_vec()).encode()?.into()))
                        .await?;
                }
                size = resize_rx.recv() => {
                    let Some(size) = size else {
                        break;
                    };
                    writer
                        .send(Message::Binary(ClientMessage::Resize(size).encode()?.into()))
                        .await?;
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    });

    let mut stdout = tokio::io::stdout();
    while let Some(message) = reader.next().await {
        match message? {
            Message::Binary(frame) => match ServerMessage::decode(&frame)? {
                ServerMessage::Output(output) => write_server_output(&mut stdout, &output).await?,
                ServerMessage::Error(error) => bail!(error),
            },
            Message::Text(text) => write_server_output(&mut stdout, text.as_bytes()).await?,
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
        }
    }

    writer_task.abort();
    Ok(())
}

async fn write_server_output<W>(stdout: &mut W, output: &[u8]) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    stdout.write_all(output).await?;
    stdout.flush().await?;
    Ok(())
}

pub fn resolve_ws_url(connect: &str) -> Result<Url> {
    let mut base = Url::parse(connect).context("parse connect URL")?;
    if !base.path().ends_with('/') {
        base.set_path(&format!("{}/", base.path()));
    }
    let mut url = base.join("ws").context("resolve websocket URL")?;
    match url.scheme() {
        "http" => url
            .set_scheme("ws")
            .map_err(|_| anyhow::anyhow!("set ws scheme"))?,
        "https" => url
            .set_scheme("wss")
            .map_err(|_| anyhow::anyhow!("set wss scheme"))?,
        "ws" | "wss" => {}
        scheme => bail!("unsupported connect URL scheme {scheme}"),
    }
    Ok(url)
}

pub fn basic_auth_header(credential: &Option<Credential>) -> Option<String> {
    credential.as_ref().map(|credential| {
        let pair = credential.to_basic_pair();
        format!("Basic {}", STANDARD.encode(pair))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    use terminald_protocol::{ClientMessage, Resize};
    use tokio::io::AsyncWrite;

    #[derive(Default)]
    struct RecordingStdout {
        bytes: Vec<u8>,
        flushes: usize,
    }

    impl AsyncWrite for RecordingStdout {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            self.bytes.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::io::Result<()>> {
            self.flushes += 1;
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn resolves_client_urls() {
        assert_eq!(
            resolve_ws_url("http://127.0.0.1:7681").unwrap().as_str(),
            "ws://127.0.0.1:7681/ws"
        );
        assert_eq!(
            resolve_ws_url("https://site.com/example/bbb/")
                .unwrap()
                .as_str(),
            "wss://site.com/example/bbb/ws"
        );
    }

    #[test]
    fn creates_basic_auth_header() {
        let credential = Some(Credential::new("user:pass").unwrap());
        assert_eq!(
            basic_auth_header(&credential).unwrap(),
            "Basic dXNlcjpwYXNz"
        );
    }

    #[test]
    fn encodes_initial_and_changed_resize_frames() {
        let first = ClientMessage::Resize(Resize {
            cols: 100,
            rows: 40,
        })
        .encode()
        .unwrap();
        assert_eq!(
            ClientMessage::decode(&first).unwrap(),
            ClientMessage::Resize(Resize {
                cols: 100,
                rows: 40
            })
        );
        let second = ClientMessage::Resize(Resize {
            cols: 120,
            rows: 50,
        })
        .encode()
        .unwrap();
        assert_eq!(
            ClientMessage::decode(&second).unwrap(),
            ClientMessage::Resize(Resize {
                cols: 120,
                rows: 50
            })
        );
    }

    #[tokio::test]
    async fn server_output_flushes_immediately() {
        let mut stdout = RecordingStdout::default();

        write_server_output(&mut stdout, b"a").await.unwrap();

        assert_eq!(stdout.bytes, b"a");
        assert_eq!(stdout.flushes, 1);
    }
}
