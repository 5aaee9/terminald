use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use terminald_protocol::{ClientMessage, Resize, ServerMessage};
use terminald_pty::{PtyCommand, PtyProcess, PtySize};
use tokio::sync::mpsc;

pub async fn handle_socket(socket: WebSocket, command: Vec<String>) {
    if let Err(error) = run_socket(socket, command).await {
        eprintln!("{error:#}");
    }
}

async fn run_socket(socket: WebSocket, command: Vec<String>) -> anyhow::Result<()> {
    let process = PtyProcess::spawn(PtyCommand::new(command), PtySize::default()).await?;
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(32);
    let reader_process = process.clone_handle();
    let writer_process = process.clone_handle();

    tokio::spawn(async move {
        let mut buf = [0_u8; 8192];
        loop {
            match reader_process.read(&mut buf).await {
                Ok(0) => break,
                Ok(read) => {
                    if tx
                        .send(ServerMessage::Output(buf[..read].to_vec()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    let _ = tx.send(ServerMessage::Error(format!("{error:#}"))).await;
                    break;
                }
            }
        }
    });

    loop {
        tokio::select! {
            Some(message) = rx.recv() => {
                sender.send(Message::Binary(message.encode().into())).await?;
            }
            Some(message) = receiver.next() => {
                match message? {
                    Message::Binary(frame) => {
                        match ClientMessage::decode(&frame) {
                            Ok(ClientMessage::Input(data)) => writer_process.write_all(&data).await?,
                            Ok(ClientMessage::Resize(Resize { cols, rows })) => writer_process.resize(cols, rows)?,
                            Err(error) => {
                                sender.send(Message::Binary(ServerMessage::Error(format!("{error:#}")).encode().into())).await?;
                            }
                        }
                    }
                    Message::Text(text) => {
                        writer_process.write_all(text.as_bytes()).await?;
                    }
                    Message::Close(_) => break,
                    Message::Ping(data) => sender.send(Message::Pong(data)).await?,
                    Message::Pong(_) => {}
                }
            }
            else => break,
        }
    }
    drop(process);
    Ok(())
}
