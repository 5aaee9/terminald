use std::{fmt, path::PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;
use terminald_server::Credential;

#[derive(Args)]
pub(super) struct CredentialArgs {
    #[arg(
        short = 'c',
        long = "credential",
        value_name = "USER:PASSWORD",
        help = "Basic authentication credential"
    )]
    credential: Option<String>,

    #[arg(
        long,
        value_name = "PATH",
        conflicts_with = "credential",
        help = "Read a UTF-8 user:password credential from a file"
    )]
    credential_file: Option<PathBuf>,
}

impl fmt::Debug for CredentialArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredentialArgs")
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "[REDACTED]"),
            )
            .field("credential_file", &self.credential_file)
            .finish()
    }
}

impl CredentialArgs {
    pub(super) fn resolve(self) -> Result<Option<Credential>> {
        let value = match self.credential_file {
            Some(path) => {
                let value = std::fs::read_to_string(&path)
                    .with_context(|| format!("read credential file {}", path.display()))?;
                let line = value
                    .strip_suffix("\r\n")
                    .or_else(|| value.strip_suffix('\n'))
                    .unwrap_or(&value);
                if line.contains(['\r', '\n', '\0']) {
                    bail!("credential file must contain a single user:password line");
                }
                Some(line.to_owned())
            }
            None => self.credential,
        };
        value
            .map(|value| {
                Credential::new(&value)
                    .ok_or_else(|| anyhow::anyhow!("credential must be user:password"))
            })
            .transpose()
    }
}
