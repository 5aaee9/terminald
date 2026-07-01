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

#[tokio::test]
async fn drop_reaps_process_group_children_that_ignore_sigterm() {
    let dir = tempfile::tempdir().unwrap();
    let ready = dir.path().join("ready");
    let child_pid = dir.path().join("child-pid");
    let script = format!(
        "sh -c 'trap \"\" TERM; trap \"\" HUP; echo $$ > {child_pid}; touch {ready}; while true; do sleep 1; done' & wait",
        child_pid = child_pid.display(),
        ready = ready.display(),
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
    let pid = tokio::fs::read_to_string(&child_pid)
        .await
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    drop(process);

    for _ in 0..50 {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("background child process that ignores SIGTERM was not reaped");
}
