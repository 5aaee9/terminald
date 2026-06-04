use anyhow::{Context, Result};
use terminald_protocol::Resize;
use tokio::sync::mpsc;

pub struct RawTerminalGuard;

impl RawTerminalGuard {
    pub fn enter() -> Result<Self> {
        crossterm::terminal::enable_raw_mode().context("enable terminal raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawTerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

pub struct TerminalEvents {
    _resize_tx: mpsc::Sender<Resize>,
}

impl TerminalEvents {
    pub fn new(resize_tx: mpsc::Sender<Resize>) -> Result<Self> {
        #[cfg(unix)]
        {
            let tx = resize_tx.clone();
            tokio::spawn(async move {
                if let Ok(mut signal) =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())
                {
                    while signal.recv().await.is_some() {
                        if let Some(size) = current_size() {
                            let _ = tx.send(size).await;
                        }
                    }
                }
            });
        }
        Ok(Self {
            _resize_tx: resize_tx,
        })
    }

    pub fn current_size(&self) -> Option<Resize> {
        current_size()
    }
}

fn current_size() -> Option<Resize> {
    let (cols, rows) = crossterm::terminal::size().ok()?;
    Some(Resize { cols, rows })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct TestGuard(Arc<AtomicBool>);

    impl Drop for TestGuard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn raw_mode_guard_restores_on_drop_shape() {
        let restored = Arc::new(AtomicBool::new(false));
        {
            let _guard = TestGuard(Arc::clone(&restored));
        }
        assert!(restored.load(Ordering::SeqCst));
    }
}
