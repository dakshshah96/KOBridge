//! SSH setup help for the Settings view: key, `Host kindle` block, USB key install, first connection.

use crate::ssh::{tr, Res, HOST};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const KEY: &str = ".ssh/id_ed25519_kindle";
const AUTHORIZED: &str = "koreader/settings/SSH/authorized_keys";

#[derive(Serialize)]
pub struct Info {
    key_path: String,
    public_key: Option<String>,
    config_path: String,
    /// `~/.ssh/config` names `kindle` in a `Host` or `Match originalhost` line
    has_host: bool,
    /// `ssh -G kindle` values, empty when the alias isn't configured
    hostname: String,
    port: String,
    user: String,
    identity: String,
    /// mounted Kindle USB volume, if any
    usb: Option<String>,
}

/// True when a `Host` or `Match originalhost` line names the alias.
pub fn names_kindle(config: &str) -> bool {
    config.lines().any(|l| {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first().map(|s| s.to_ascii_lowercase()) {
            Some(k) if k == "host" => w[1..].contains(&HOST),
            Some(k) if k == "match" => w.windows(2).any(|p| p[0].eq_ignore_ascii_case("originalhost") && p[1] == HOST),
            _ => false,
        }
    })
}

pub fn host_block(ip: &str, port: u16) -> String {
    format!("Host {HOST}\n  HostName {ip}\n  Port {port}\n  User root\n  IdentityFile ~/{KEY}\n  IdentitiesOnly yes\n\n")
}

pub fn valid_host(ip: &str) -> bool {
    !ip.is_empty() && ip.len() <= 253 && ip.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-'))
}

fn usb_volume(home: &Path) -> Option<PathBuf> {
    let user = home.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    [PathBuf::from("/Volumes/Kindle"), PathBuf::from(format!("/media/{user}/Kindle")), PathBuf::from(format!("/run/media/{user}/Kindle"))]
        .into_iter().find(|p| p.join("koreader").is_dir())
}

pub async fn info(home: &Path) -> Info {
    let key = home.join(KEY);
    let config = home.join(".ssh/config");
    let text = std::fs::read_to_string(&config).unwrap_or_default();
    let g = tokio::process::Command::new("/usr/bin/ssh").args(["-G", HOST]).output().await
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let get = |k: &str| g.lines().find_map(|l| l.strip_prefix(k).and_then(|v| v.strip_prefix(' '))).unwrap_or("").to_string();
    let has_host = names_kindle(&text);
    Info {
        key_path: key.to_string_lossy().into(),
        public_key: std::fs::read_to_string(key.with_extension("pub")).ok().map(|s| s.trim().to_string()),
        config_path: config.to_string_lossy().into(),
        has_host,
        hostname: if has_host { get("hostname") } else { String::new() },
        port: if has_host { get("port") } else { String::new() },
        user: if has_host { get("user") } else { String::new() },
        identity: if has_host { get("identityfile") } else { String::new() },
        usb: usb_volume(home).map(|p| p.to_string_lossy().into()),
    }
}

pub async fn create_key(home: &Path) -> Res<()> {
    let key = home.join(KEY);
    if key.exists() { return Err(tr(format!("{} already exists.", key.display()), format!("{} ya existe.", key.display()))); }
    std::fs::create_dir_all(key.parent().unwrap()).map_err(|e| e.to_string())?;
    let out = tokio::process::Command::new("/usr/bin/ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", HOST, "-f"]).arg(&key)
        .output().await.map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// Puts a `Host kindle` block at the top of ~/.ssh/config, after copying the file to config.kob.bak.
pub fn add_host(home: &Path, ip: &str, port: u16) -> Res<()> {
    let ip = ip.trim();
    if !valid_host(ip) { return Err(tr("Type an IP address like 192.168.1.50.", "Escribí una IP como 192.168.1.50.")); }
    let dir = home.join(".ssh");
    let config = dir.join("config");
    let old = std::fs::read_to_string(&config).unwrap_or_default();
    if names_kindle(&old) {
        return Err(tr("~/.ssh/config already has a kindle entry. Edit it there.", "~/.ssh/config ya tiene una entrada kindle. Editala ahí."));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if config.exists() { std::fs::copy(&config, dir.join("config.kob.bak")).map_err(|e| e.to_string())?; }
    // first match wins in ssh_config, so the block goes before any `Host *`
    std::fs::write(&config, host_block(ip, port) + &old).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Appends the public key to authorized_keys on a Kindle mounted over USB.
pub fn install_usb(home: &Path) -> Res<String> {
    let vol = usb_volume(home).ok_or_else(|| tr("Connect the Kindle over USB first.", "Primero conectá la Kindle por USB."))?;
    let key = std::fs::read_to_string(home.join(KEY).with_extension("pub"))
        .map_err(|_| tr("Create the key first.", "Primero creá la llave."))?;
    let key = key.trim();
    let path = vol.join(AUTHORIZED);
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    if !old.lines().any(|l| l.trim() == key) {
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let sep = if old.is_empty() || old.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&path, format!("{old}{sep}{key}\n")).map_err(|e| e.to_string())?;
    }
    Ok(path.to_string_lossy().into())
}

/// First connection through the alias; accepts and saves the host key if this is the first time.
pub async fn test() -> Res<String> {
    let out = tokio::process::Command::new("/usr/bin/ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", "-o", "StrictHostKeyChecking=accept-new", HOST, "cat /mnt/us/koreader/git-rev"])
        .stdin(std::process::Stdio::null())
        .output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // with `Match originalhost kindle exec ...` entries, every probe failing leaves the alias unresolved
        let config = std::fs::read_to_string(crate::home().join(".ssh/config")).unwrap_or_default();
        if err.contains("Could not resolve hostname") && names_kindle(&config) { Err(crate::ssh::unreachable()) } else { Err(err) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_kindle_entries() {
        assert!(names_kindle("Host kindle\n  Port 2222\n"));
        assert!(names_kindle("Host foo kindle\n"));
        assert!(names_kindle("Match originalhost kindle exec \"nc -z 1.2.3.4 2222\"\n"));
        assert!(!names_kindle("Host kindle2\nHost *\n  User x\n"));
        assert!(!names_kindle("# Host kindle\n"));
    }

    #[test]
    fn writes_host_block_and_checks_ip() {
        assert!(host_block("192.168.1.50", 2222).starts_with("Host kindle\n  HostName 192.168.1.50\n  Port 2222\n"));
        assert!(valid_host("192.168.1.50") && valid_host("fe80::1") && valid_host("kindle.local"));
        assert!(!valid_host("1.2.3.4 -oProxyCommand=x") && !valid_host("a\nHost *") && !valid_host(""));
    }

    #[test]
    fn add_host_backs_up_and_prepends() {
        let home = std::env::temp_dir().join("kob-setup-test");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(home.join(".ssh/config"), "Host *\n  User me\n").unwrap();
        add_host(&home, "10.0.0.7", 2222).unwrap();
        let c = std::fs::read_to_string(home.join(".ssh/config")).unwrap();
        assert!(c.starts_with("Host kindle\n") && c.ends_with("Host *\n  User me\n"), "{c}");
        assert_eq!(std::fs::read_to_string(home.join(".ssh/config.kob.bak")).unwrap(), "Host *\n  User me\n");
        assert!(add_host(&home, "10.0.0.8", 2222).is_err(), "second entry refused");
    }
}
