use std::{
    fs::File,
    io::ErrorKind,
    os::{
        fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd},
        unix::process::CommandExt,
    },
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use nix::{
    errno::Errno,
    pty::Winsize,
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use tokio::{io::unix::AsyncFd, task};

const PTY_TERMINATE_GRACE_PERIOD: Duration = Duration::from_millis(200);
const PTY_TERMINATE_POLL_INTERVAL: Duration = Duration::from_millis(10);

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
    io: Arc<AsyncFd<File>>,
    child: Arc<Mutex<Child>>,
    cleanup_started: Arc<AtomicBool>,
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
        if !self.inner.start_cleanup() {
            return Ok(());
        }
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
        loop {
            let mut guard = self.io.readable().await.context("wait for PTY readable")?;
            match guard.try_io(|inner| read_pty(inner.get_ref().as_raw_fd(), buf)) {
                Ok(Ok(read)) => return Ok(read),
                Ok(Err(error)) if is_pty_eof(&error) => return Ok(0),
                Ok(Err(error)) => return Err(error).context("read PTY master"),
                Err(_would_block) => continue,
            }
        }
    }

    pub async fn write_all(&self, data: &[u8]) -> Result<()> {
        let mut written = 0;
        while written < data.len() {
            let mut guard = self.io.writable().await.context("wait for PTY writable")?;
            match guard.try_io(|inner| write_pty(inner.get_ref().as_raw_fd(), &data[written..])) {
                Ok(Ok(0)) => bail!("write PTY master returned zero bytes"),
                Ok(Ok(count)) => written += count,
                Ok(Err(error)) => return Err(error).context("write PTY master"),
                Err(_would_block) => continue,
            }
        }
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let fd = self.io.get_ref().as_raw_fd();
        let result = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &size) };
        if result == -1 {
            return Err(std::io::Error::last_os_error()).context("resize PTY");
        }
        Ok(())
    }

    fn terminate_blocking_best_effort(&self) {
        if !self.start_cleanup() {
            return;
        }
        let child = Arc::clone(&self.child);
        let _ = thread::Builder::new()
            .name("terminald-pty-cleanup".into())
            .spawn(move || {
                let Ok(mut child) = child.lock() else {
                    return;
                };
                let _ = terminate_child_process_group_blocking(&mut child);
            });
    }

    fn start_cleanup(&self) -> bool {
        !self.cleanup_started.swap(true, Ordering::AcqRel)
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

fn read_pty(fd: RawFd, buf: &mut [u8]) -> std::io::Result<usize> {
    loop {
        let result = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if result >= 0 {
            return Ok(result as usize);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

fn write_pty(fd: RawFd, buf: &[u8]) -> std::io::Result<usize> {
    loop {
        let result = unsafe { libc::write(fd, buf.as_ptr().cast(), buf.len()) };
        if result >= 0 {
            return Ok(result as usize);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

fn set_nonblocking(fd: RawFd) -> Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(std::io::Error::last_os_error()).context("get PTY master flags");
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(std::io::Error::last_os_error()).context("set PTY master nonblocking");
    }
    Ok(())
}

fn signal_process_group(child_id: u32, signal: Signal, action: &str) -> Result<()> {
    match kill(Pid::from_raw(-(child_id as i32)), signal) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(error).with_context(|| format!("{action} PTY process group {child_id}")),
    }
}

fn terminate_process_group(child_id: u32) -> Result<()> {
    signal_process_group(child_id, Signal::SIGTERM, "terminate")
}

fn kill_process_group(child_id: u32) -> Result<()> {
    signal_process_group(child_id, Signal::SIGKILL, "kill")
}

fn wait_for_child_exit_blocking(child: &mut Child, timeout: Duration) -> Result<bool> {
    let deadline = Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .context("check PTY child status after terminate")?
            .is_some()
        {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(PTY_TERMINATE_POLL_INTERVAL);
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
    let child_exited = wait_for_child_exit_blocking(child, PTY_TERMINATE_GRACE_PERIOD)?;
    kill_process_group(child_id)?;
    if child_exited {
        return Ok(());
    }
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
    set_nonblocking(master_file.as_raw_fd())?;
    let io = Arc::new(AsyncFd::new(master_file).context("register PTY master")?);
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
            io,
            child: Arc::new(Mutex::new(child)),
            cleanup_started: Arc::new(AtomicBool::new(false)),
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
mod tests;
