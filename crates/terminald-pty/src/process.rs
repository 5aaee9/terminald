use std::{
    fs::File,
    io::{ErrorKind, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, anyhow, bail};
use nix::{
    errno::Errno,
    pty::Winsize,
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use tokio::task;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyCommand {
    pub argv: Vec<String>,
}

impl PtyCommand {
    pub fn new(argv: Vec<String>) -> Self {
        Self { argv }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
}

impl Default for PtySize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

#[derive(Debug)]
pub struct PtyProcess {
    inner: PtyHandle,
}

#[derive(Debug, Clone)]
pub struct PtyHandle {
    reader: Arc<Mutex<File>>,
    writer: Arc<Mutex<File>>,
    child: Arc<Mutex<Child>>,
}

impl PtyProcess {
    pub async fn spawn(command: PtyCommand, size: PtySize) -> Result<Self> {
        let argv = command.argv;
        if argv.is_empty() {
            bail!("pty command is empty");
        }

        task::spawn_blocking(move || spawn_blocking(argv, size))
            .await
            .context("join PTY spawn task")?
    }

    pub async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        self.inner.read(buf).await
    }

    pub async fn write_all(&self, data: &[u8]) -> Result<()> {
        self.inner.write_all(data).await
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.inner.resize(cols, rows)
    }

    pub fn clone_handle(&self) -> PtyHandle {
        self.inner.clone()
    }

    pub async fn terminate(&mut self) -> Result<()> {
        let child = Arc::clone(&self.inner.child);
        task::spawn_blocking(move || {
            let mut child = child
                .lock()
                .map_err(|_| anyhow!("PTY child lock poisoned"))?;
            terminate_child_process_group_blocking(&mut child)
        })
        .await
        .context("join PTY terminate task")?
    }

    pub async fn wait(&self) -> Result<ExitStatus> {
        self.inner.wait().await
    }

    pub fn child_id(&self) -> Result<u32> {
        self.inner.child_id()
    }
}

impl PtyHandle {
    pub async fn wait(&self) -> Result<ExitStatus> {
        let child = Arc::clone(&self.child);
        task::spawn_blocking(move || {
            child
                .lock()
                .map_err(|_| anyhow!("PTY child lock poisoned"))?
                .wait()
                .context("wait for PTY child")
        })
        .await
        .context("join PTY wait task")?
    }

    pub fn child_id(&self) -> Result<u32> {
        self.child
            .lock()
            .map_err(|_| anyhow!("PTY child lock poisoned"))
            .map(|child| child.id())
    }

    pub async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let len = buf.len();
        let reader = Arc::clone(&self.reader);
        let data = task::spawn_blocking(move || {
            let mut data = vec![0_u8; len];
            let mut file = reader
                .lock()
                .map_err(|_| anyhow!("PTY reader lock poisoned"))?;
            let read = match file.read(&mut data) {
                Ok(read) => read,
                Err(error) if is_pty_eof(&error) => 0,
                Err(error) => return Err(error).context("read PTY master"),
            };
            data.truncate(read);
            Ok::<_, anyhow::Error>(data)
        })
        .await
        .context("join PTY read task")??;

        let read = data.len();
        buf[..read].copy_from_slice(&data);
        Ok(read)
    }

    pub async fn write_all(&self, data: &[u8]) -> Result<()> {
        let data = data.to_vec();
        let writer = Arc::clone(&self.writer);
        task::spawn_blocking(move || {
            let mut file = writer
                .lock()
                .map_err(|_| anyhow!("PTY writer lock poisoned"))?;
            file.write_all(&data).context("write PTY master")?;
            file.flush().context("flush PTY master")
        })
        .await
        .context("join PTY write task")?
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let fd = self
            .writer
            .lock()
            .map_err(|_| anyhow!("PTY resize lock poisoned"))?
            .as_raw_fd();
        let result = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &size) };
        if result == -1 {
            return Err(std::io::Error::last_os_error()).context("resize PTY");
        }
        Ok(())
    }

    fn terminate_blocking_best_effort(&self) {
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        let _ = terminate_child_process_group_blocking(&mut child);
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        self.inner.terminate_blocking_best_effort();
    }
}

fn is_pty_eof(error: &std::io::Error) -> bool {
    error.kind() == ErrorKind::UnexpectedEof || error.raw_os_error() == Some(Errno::EIO as i32)
}

fn terminate_process_group(child_id: u32) -> Result<()> {
    match kill(Pid::from_raw(-(child_id as i32)), Signal::SIGTERM) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(error).with_context(|| format!("terminate PTY process group {child_id}")),
    }
}

fn terminate_child_process_group_blocking(child: &mut Child) -> Result<()> {
    if child
        .try_wait()
        .context("check PTY child status before terminate")?
        .is_some()
    {
        return Ok(());
    }
    let child_id = child.id();
    terminate_process_group(child_id)?;
    child.wait().context("wait for PTY child after terminate")?;
    Ok(())
}

fn spawn_blocking(argv: Vec<String>, size: PtySize) -> Result<PtyProcess> {
    let winsize = Winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = nix::pty::openpty(Some(&winsize), None).context("open PTY")?;
    let master_file = unsafe { File::from_raw_fd(pty.master.into_raw_fd()) };
    let writer_file = master_file
        .try_clone()
        .context("clone PTY master for writing")?;
    let reader = Arc::new(Mutex::new(master_file));
    let writer = Arc::new(Mutex::new(writer_file));
    let slave = pty.slave;

    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    unsafe {
        command.pre_exec(move || child_setup(&slave));
    }

    let child = command
        .spawn()
        .with_context(|| format!("spawn PTY command {}", argv[0]))?;
    Ok(PtyProcess {
        inner: PtyHandle {
            reader,
            writer,
            child: Arc::new(Mutex::new(child)),
        },
    })
}

fn child_setup(slave: &OwnedFd) -> std::io::Result<()> {
    if unsafe { libc::setsid() } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { libc::ioctl(slave.as_raw_fd(), libc::TIOCSCTTY, 0) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    for fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
        if unsafe { libc::dup2(slave.as_raw_fd(), fd) } == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }
    if unsafe { libc::setenv(c"TERM".as_ptr(), c"xterm-256color".as_ptr(), 1) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokio::time::{Duration, timeout};

    async fn wait_for_file(path: &Path) -> bool {
        for _ in 0..50 {
            if tokio::fs::try_exists(path).await.unwrap() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    async fn wait_until_contains(path: &Path, expected: &str) -> bool {
        for _ in 0..50 {
            if let Ok(contents) = tokio::fs::read_to_string(path).await
                && contents.contains(expected)
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    #[tokio::test]
    async fn spawns_reads_writes_and_resizes() {
        let mut process = PtyProcess::spawn(
            PtyCommand::new(vec!["sh".into(), "-lc".into(), "printf ready; cat".into()]),
            PtySize::default(),
        )
        .await
        .unwrap();

        let mut buf = [0_u8; 1024];
        let read = timeout(Duration::from_secs(3), process.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert!(String::from_utf8_lossy(&buf[..read]).contains("ready"));

        process.write_all(b"hello\n").await.unwrap();
        let read = timeout(Duration::from_secs(3), process.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert!(String::from_utf8_lossy(&buf[..read]).contains("hello"));

        process.resize(100, 40).unwrap();
        process.terminate().await.unwrap();
    }

    #[tokio::test]
    async fn drop_terminates_process_group_children() {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("ready");
        let trapped = dir.path().join("trapped");
        let script = format!(
            "sh -c 'trap \"echo child-term > {trapped}; exit 0\" TERM; trap \"\" HUP; touch {ready}; while true; do sleep 1; done' & wait",
            ready = ready.display(),
            trapped = trapped.display(),
        );

        let process = PtyProcess::spawn(
            PtyCommand::new(vec!["sh".into(), "-lc".into(), script]),
            PtySize::default(),
        )
        .await
        .unwrap();

        assert!(
            wait_for_file(&ready).await,
            "background child process did not become ready"
        );
        drop(process);

        assert!(
            wait_until_contains(&trapped, "child-term").await,
            "background child process did not receive SIGTERM from PTY Drop cleanup"
        );
    }
}
