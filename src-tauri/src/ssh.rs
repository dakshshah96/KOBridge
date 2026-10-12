//! Everything goes through the system `ssh kindle`, so ~/.ssh/config decides the IP
//! (Match exec probes), the key and the Keychain passphrase. ControlMaster keeps one
//! TCP connection warm so each command costs a round trip, not a handshake.

use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

pub const HOST: &str = "kindle";

pub type Res<T> = Result<T, String>;

fn ssh() -> Command {
    let mut c = Command::new("/usr/bin/ssh");
    c.args([
        "-o", "BatchMode=yes",
        "-o", "ConnectTimeout=5",
        "-o", "ServerAliveInterval=10",
        "-o", "ControlMaster=auto",
        "-o", "ControlPath=~/.ssh/kob-%C",
        "-o", "ControlPersist=120",
        HOST,
    ]);
    c.stdin(Stdio::null()).kill_on_drop(true);
    c
}

/// Single-quote for the remote POSIX shell.
pub fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn fail(stderr: &[u8], code: Option<i32>) -> String {
    let msg = String::from_utf8_lossy(stderr).trim().to_string();
    if msg.is_empty() { format!("ssh terminó con código {}", code.unwrap_or(-1)) } else { msg }
}

pub async fn run(cmd: &str) -> Res<String> {
    let out = ssh().arg(cmd).output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(fail(&out.stderr, out.status.code()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run with `input` on stdin (used to write remote files without quoting their content).
pub async fn run_with_input(cmd: &str, input: &[u8]) -> Res<()> {
    let mut child = ssh().arg(cmd).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped())
        .spawn().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input).await.map_err(|e| e.to_string())?;
    drop(stdin);
    let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(fail(&out.stderr, out.status.code()));
    }
    Ok(())
}

/// Stream remote stdout into a local file, reporting bytes written.
pub async fn pull_to_file(cmd: &str, dest: &Path, mut progress: impl FnMut(u64)) -> Res<u64> {
    let mut child = ssh().arg(cmd).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| e.to_string())?;
    let mut src = child.stdout.take().unwrap();
    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let (mut total, mut last) = (0u64, 0u64);
    loop {
        let chunk = match src.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => file.write_all(&buf[..n]).await.map(|_| n),
            Err(e) => Err(e),
        };
        let n = match chunk {
            Ok(n) => n,
            Err(e) => { let _ = tokio::fs::remove_file(&tmp).await; return Err(e.to_string()); }
        };
        total += n as u64;
        if total - last >= 1 << 20 { progress(total); last = total; }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(fail(&out.stderr, out.status.code()));
    }
    tokio::fs::rename(&tmp, dest).await.map_err(|e| e.to_string())?;
    progress(total);
    Ok(total)
}

/// Copy one local file into a remote directory. `cat > file` instead of scp because
/// scp's remote side word-splits names with spaces and parentheses.
pub async fn upload(local: &Path, remote_dir: &str) -> Res<String> {
    let name = local.file_name().and_then(|n| n.to_str()).ok_or("nombre de archivo inválido")?.to_string();
    let target = format!("{}/{}", remote_dir.trim_end_matches('/'), name);
    let file = std::fs::File::open(local).map_err(|e| e.to_string())?;
    let cmd = format!("mkdir -p {d} && cat > {t}.kob-part && mv {t}.kob-part {t}", d = sq(remote_dir), t = sq(&target));
    let out = ssh().arg(cmd).stdin(Stdio::from(file)).stdout(Stdio::null()).stderr(Stdio::piped())
        .output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(fail(&out.stderr, out.status.code()));
    }
    Ok(target)
}

/// Spawn a long-running remote command; caller reads its stdout.
pub fn stream(cmd: &str) -> Res<Child> {
    ssh().arg(cmd).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())
}

/// The IP ssh would use right now (runs the Match exec probes).
pub async fn resolved_host() -> Option<String> {
    let out = Command::new("/usr/bin/ssh").args(["-G", HOST]).output().await.ok()?;
    String::from_utf8_lossy(&out.stdout).lines()
        .find_map(|l| l.strip_prefix("hostname ").map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::sq;
    #[test]
    fn quotes_single_quotes() {
        assert_eq!(sq("it's"), r"'it'\''s'");
        assert_eq!(sq("a b (1).epub"), "'a b (1).epub'");
    }
}
