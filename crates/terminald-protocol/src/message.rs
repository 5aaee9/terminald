use anyhow::{Context, Result, anyhow, bail};

use crate::Resize;

pub const CLIENT_RESIZE: u8 = 0;
pub const CLIENT_INPUT: u8 = 1;
pub const SERVER_OUTPUT: u8 = 2;
pub const SERVER_ERROR: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    Resize(Resize),
    Input(Vec<u8>),
}

impl ClientMessage {
    pub fn decode(frame: &[u8]) -> Result<Self> {
        let (&kind, payload) = frame
            .split_first()
            .ok_or_else(|| anyhow!("empty client frame"))?;

        match kind {
            CLIENT_RESIZE => Resize::from_payload(payload)
                .map(Self::Resize)
                .context("invalid resize payload"),
            CLIENT_INPUT => Ok(Self::Input(payload.to_vec())),
            other => bail!("unknown client frame prefix {other}"),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        match self {
            Self::Resize(size) => {
                let mut frame = vec![CLIENT_RESIZE];
                frame.extend(size.to_payload()?);
                Ok(frame)
            }
            Self::Input(data) => {
                let mut frame = Vec::with_capacity(data.len() + 1);
                frame.push(CLIENT_INPUT);
                frame.extend(data);
                Ok(frame)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    Output(Vec<u8>),
    Error(String),
}

impl ServerMessage {
    pub fn decode(frame: &[u8]) -> Result<Self> {
        let (&kind, payload) = frame
            .split_first()
            .ok_or_else(|| anyhow!("empty server frame"))?;

        match kind {
            SERVER_OUTPUT => Ok(Self::Output(payload.to_vec())),
            SERVER_ERROR => Ok(Self::Error(String::from_utf8_lossy(payload).into_owned())),
            other => bail!("unknown server frame prefix {other}"),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Output(data) => {
                let mut frame = Vec::with_capacity(data.len() + 1);
                frame.push(SERVER_OUTPUT);
                frame.extend(data);
                frame
            }
            Self::Error(message) => {
                let mut frame = Vec::with_capacity(message.len() + 1);
                frame.push(SERVER_ERROR);
                frame.extend(message.as_bytes());
                frame
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_client_input() {
        assert_eq!(
            ClientMessage::decode(&[CLIENT_INPUT, b'a']).unwrap(),
            ClientMessage::Input(vec![b'a'])
        );
    }

    #[test]
    fn invalid_resize_has_boundary_context_and_cause() {
        let err = ClientMessage::decode(&[CLIENT_RESIZE, b'{', b'"', b'c']).unwrap_err();
        assert_eq!(err.to_string(), "invalid resize payload");
        assert!(format!("{err:#}").contains("EOF while parsing"));
    }

    #[test]
    fn encodes_server_messages() {
        assert_eq!(
            ServerMessage::Output(vec![b'x']).encode(),
            vec![SERVER_OUTPUT, b'x']
        );
        assert_eq!(
            ServerMessage::Error("no".to_string()).encode(),
            vec![SERVER_ERROR, b'n', b'o']
        );
    }
}
