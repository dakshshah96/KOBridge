//! Downloads through aria2c: the one on the login shell's PATH, else the copy bundled as a
//! Tauri sidecar (built by scripts/build-aria2.sh).

use crate::ssh::{tr, Res};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, BufReader};

/// CA bundles a static Linux build may not find on its own (Debian/Ubuntu, Fedora).
const CA_FILES: [&str; 2] = ["/etc/ssl/certs/ca-certificates.crt", "/etc/pki/tls/certs/ca-bundle.crt"];

pub fn find() -> Option<PathBuf> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND.get_or_init(|| {
        // a Finder-launched app gets a bare PATH, so ask the login shell (same as find_claude)
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let on_path = std::process::Command::new(shell).args(["-lc", "command -v aria2c"]).stdin(Stdio::null()).output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|p| p.starts_with('/'))
            .map(PathBuf::from);
        on_path.or_else(sidecar)
    }).clone()
}

/// Tauri copies `externalBin` next to the main executable, without the target triple.
fn sidecar() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.parent()?.join("aria2c");
    p.is_file().then_some(p)
}

pub fn args(url: &str, dir: &Path, out: &str) -> Vec<String> {
    let mut a: Vec<String> = [
        "--no-conf=true",
        "-x", "8", "-s", "8", "-k", "1M",
        "--file-allocation=none",
        "--continue=true", "--max-tries=5", "--retry-wait=2",
        "--connect-timeout=10", "--timeout=30",
        "--auto-file-renaming=false", "--allow-overwrite=true",
        "--http-accept-gzip=true",
        "--summary-interval=1", "--show-console-readout=false", "--enable-color=false",
        "--download-result=hide", "--console-log-level=warn",
    ].iter().map(|s| s.to_string()).collect();
    a.push(format!("--user-agent=KOBridge/{}", env!("CARGO_PKG_VERSION")));
    if !cfg!(target_os = "macos") {
        if let Some(ca) = CA_FILES.iter().find(|p| Path::new(p).is_file()) { a.push(format!("--ca-certificate={ca}")); }
    }
    a.extend(["-d".into(), dir.to_string_lossy().into(), "-o".into(), out.into(), url.into()]);
    a
}

fn bytes(s: &str) -> Option<u64> {
    let (num, mult) = [("GiB", 1u64 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10), ("B", 1)]
        .iter().find_map(|(u, m)| s.strip_suffix(u).map(|n| (n, *m)))?;
    Some((num.parse::<f64>().ok()? * mult as f64) as u64)
}

/// `[#2089b0 1.2MiB/5.8MiB(20%) CN:8 DL:2.1MiB ETA:2s]` → (done, total)
pub fn progress(line: &str) -> Option<(u64, u64)> {
    let l = line.trim().strip_prefix("[#")?;
    let sizes = l.split_whitespace().nth(1)?;
    let (done, rest) = sizes.split_once('/')?;
    let total = rest.split('(').next()?;
    Some((bytes(done)?, bytes(total)?))
}

/// Downloads `url` to `dir/out`, reporting (done, total) bytes about once a second.
pub async fn download(url: &str, dir: &Path, out: &str, mut on: impl FnMut(u64, u64)) -> Res<PathBuf> {
    let bin = find().ok_or_else(|| tr(
        "aria2c isn't installed and this build doesn't include it. Install it with `brew install aria2` or `sudo apt install aria2`.",
        "No encontré aria2c y esta versión no lo trae. Instalalo con `brew install aria2` o `sudo apt install aria2`."))?;
    // URLs come from the catalog: one starting with `-` would be read as an option like --on-download-complete
    if !url.starts_with("https://") || url.chars().any(char::is_whitespace) {
        return Err(tr(format!("Not an https URL: {url}"), format!("No es una URL https: {url}")));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut child = tokio::process::Command::new(bin).args(args(url, dir, out))
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .kill_on_drop(true).spawn().map_err(|e| e.to_string())?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut errors = vec![];
    while let Ok(Some(l)) = lines.next_line().await {
        if let Some((d, t)) = progress(&l) { on(d, t); } else if l.contains("[ERROR]") || l.contains("Exception:") { errors.push(l.trim().to_string()); }
    }
    let o = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if !o.status.success() {
        let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
        let why = errors.last().cloned().filter(|s| !s.is_empty()).unwrap_or(stderr);
        return Err(tr(format!("Couldn't download {url}: {why}"), format!("No pude bajar {url}: {why}")));
    }
    let p = dir.join(out);
    let n = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
    on(n, n);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_progress_lines() {
        assert_eq!(progress("[#2089b0 1.2MiB/5.8MiB(20%) CN:8 DL:2.1MiB ETA:2s]"), Some((1258291, 6081740)));
        assert_eq!(progress("[#a1 512KiB/1.0MiB(50%) CN:1 DL:0B]"), Some((524288, 1048576)));
        assert_eq!(progress("[#a1 0B/0B CN:1 DL:0B]"), Some((0, 0)));
        assert_eq!(progress("FILE: /tmp/x.zip"), None);
        assert_eq!(progress("*** Download Progress Summary as of Mon Oct 13 ***"), None);
    }

    #[test]
    fn builds_args() {
        let a = args("https://x/y.zip", Path::new("/tmp/d"), "y.zip");
        assert_eq!(a[0], "--no-conf=true");
        assert!(a.windows(2).any(|w| w[0] == "-k" && w[1] == "1M"));
        assert_eq!(&a[a.len() - 5..], ["-d", "/tmp/d", "-o", "y.zip", "https://x/y.zip"]);
    }

    #[test]
    fn refuses_non_https_urls() {
        tauri::async_runtime::block_on(async {
            for u in ["--on-download-complete=/bin/sh", "http://x/y.zip", "https://x/a b"] {
                assert!(download(u, Path::new("/tmp/kob-aria-x"), "y", |_, _| {}).await.is_err(), "{u}");
            }
        });
    }

    /// Network and aria2c: `cargo test aria_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn aria_live() {
        tauri::async_runtime::block_on(async {
            println!("aria2c: {:?}", find());
            let dir = std::env::temp_dir().join("kob-aria");
            let p = download("https://ultimatejimmy.github.io/storefront.koplugin/catalog.json", &dir, "catalog.json",
                |d, t| println!("{d}/{t}")).await.expect("download");
            assert!(std::fs::metadata(p).unwrap().len() > 500_000);
        });
    }
}
