use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resize {
    pub cols: u16,
    pub rows: u16,
}

impl Resize {
    pub fn from_payload(payload: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(payload)?)
    }

    pub fn to_payload(self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_resize_payload() {
        assert_eq!(
            Resize { cols: 80, rows: 24 }.to_payload().unwrap(),
            br#"{"cols":80,"rows":24}"#
        );
    }

    #[test]
    fn decodes_resize_payload() {
        assert_eq!(
            Resize::from_payload(br#"{"cols":120,"rows":30}"#).unwrap(),
            Resize {
                cols: 120,
                rows: 30
            }
        );
    }
}
