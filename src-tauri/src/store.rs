//! Store view: Storefront's catalog, its install registry (koreader/settings/Storefront_installs.lua)
//! and installing, updating or removing plugins, patches and fonts on the Kindle.

use crate::lua;
use crate::ssh::{self, sq, tr, Res};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const CATALOG_URL: &str = "https://ultimatejimmy.github.io/storefront.koplugin/catalog.json";
const KO: &str = "/mnt/us/koreader";
const REGISTRY: &str = "/mnt/us/koreader/settings/Storefront_installs.lua";
const SEP: &str = "@@KOB@@";

// ---------- registry ----------

/// The `installs` JSON inside Storefront_installs.lua, with its four tables always present as objects.
/// Storefront's JSON encoder writes an empty table as `[]`, so arrays are read as empty objects.
pub fn read_registry(src: &str) -> Res<Value> {
    let mut v = if src.trim().is_empty() {
        json!({})
    } else {
        let t = lua::parse(src)?;
        match t.str("installs") {
            Some(s) if !s.trim().is_empty() => serde_json::from_str(s).map_err(|e| format!("Storefront_installs.lua: {e}"))?,
            _ => json!({}),
        }
    };
    let o = v.as_object_mut().ok_or("Storefront_installs.lua: installs isn't an object")?;
    for k in ["plugins", "patches", "fonts", "item_options"] {
        if !o.get(k).is_some_and(Value::is_object) { o.insert(k.into(), json!({})); }
    }
    Ok(v)
}

/// Lua's `%q`, which KOReader's LuaSettings uses for strings.
pub fn lua_quote(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\\n"),
            '\r' => o.push_str("\\r"),
            '\0' => o.push_str("\\000"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

pub fn write_registry(v: &Value) -> String {
    format!("-- ./settings/Storefront_installs.lua\nreturn {{\n    [\"installs\"] = {},\n}}\n", lua_quote(&v.to_string()))
}

fn table<'a>(reg: &'a mut Value, k: &str) -> &'a mut Map<String, Value> {
    reg[k].as_object_mut().expect("read_registry makes it an object")
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// `2026-10-01T12:00:00Z` → unix seconds (UTC); None for anything else.
fn iso_secs(s: &str) -> Option<i64> {
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d, hh, mm, ss) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    // days from civil (Howard Hinnant)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + hh * 3600 + mm * 60 + ss)
}

// ---------- catalog ----------

fn s<'a>(v: &'a Value, k: &str) -> &'a str { v[k].as_str().unwrap_or("") }

/// What the list needs, about a third of the catalog's size.
pub fn slim(c: &Value) -> Value {
    let arr = |k: &str| c[k].as_array().cloned().unwrap_or_default();
    let common = |p: &Value| json!({
        "owner": s(p, "owner"), "description": s(p, "description"),
        "stars": p["stars"].as_u64().or(p["stargazers_count"].as_u64()).unwrap_or(0),
        "pushed_at": s(p, "pushed_at"), "url": s(p, "html_url"),
    });
    let merge = |mut a: Value, b: Value| { a.as_object_mut().unwrap().extend(b.as_object().unwrap().clone()); a };
    let plugins: Vec<Value> = arr("plugins").iter().map(|p| merge(common(p), json!({
        "id": s(p, "full_name"), "name": s(p, "name").trim_end_matches(".koplugin"),
        "version": p["latest_release"]["tag_name"].as_str().unwrap_or(""),
    }))).collect();
    let patches: Vec<Value> = arr("patches").iter().flat_map(|p| {
        p["patch_files"].as_array().cloned().unwrap_or_default().into_iter().map(move |f| merge(common(p), json!({
            "id": format!("{}/{}", s(p, "full_name"), s(&f, "path")), "name": s(&f, "filename"), "repo": s(p, "name"),
        })))
    }).collect();
    let fonts: Vec<Value> = arr("fonts").iter().map(|f| merge(common(f), json!({
        "id": s(f, "name"), "name": s(f, "name"), "category": s(f, "category"),
    }))).collect();
    json!({ "generated_at": c["generated_at"], "plugins": plugins, "patches": patches, "fonts": fonts })
}

fn find_plugin_entry<'a>(c: &'a Value, id: &str) -> Option<&'a Value> {
    c["plugins"].as_array()?.iter().find(|p| s(p, "full_name") == id)
}

/// (repo entry, patch file) for `<owner>/<repo>/<path>`.
fn find_patch_entry<'a>(c: &'a Value, id: &str) -> Option<(&'a Value, &'a Value)> {
    c["patches"].as_array()?.iter().find_map(|p| {
        let path = id.strip_prefix(s(p, "full_name"))?.strip_prefix('/')?;
        Some((p, p["patch_files"].as_array()?.iter().find(|f| s(f, "path") == path)?))
    })
}

fn find_font_entry<'a>(c: &'a Value, id: &str) -> Option<&'a Value> {
    c["fonts"].as_array()?.iter().find(|f| s(f, "name") == id)
}

/// The release zip when there is one, else the default branch, as Storefront does.
pub fn plugin_zip_url(p: &Value) -> String {
    match p["latest_release"]["download_url"].as_str() {
        Some(u) if !u.is_empty() => u.into(),
        _ => {
            let branch = match s(p, "default_branch") { "" => "main", b => b };
            format!("https://github.com/{}/archive/refs/heads/{branch}.zip", s(p, "full_name"))
        }
    }
}

// ---------- local files ----------

pub fn unzip(zip: &Path, dest: &Path) -> Res<()> {
    let f = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut a = zip::ZipArchive::new(f).map_err(|e| tr(format!("Not a zip file: {e}"), format!("No es un zip: {e}")))?;
    // entries like ../x are rejected by enclosed_name before anything is written
    for i in 0..a.len() {
        let name = a.by_index(i).map_err(|e| e.to_string())?.enclosed_name();
        if name.is_none() { return Err(tr("The zip has a path outside its folder.", "El zip tiene una ruta fuera de su carpeta.")); }
    }
    a.extract(dest).map_err(|e| e.to_string())
}

/// The plugin folder inside an unzipped archive and the name it gets on the Kindle.
pub fn find_plugin(root: &Path, repo: &str) -> Res<(PathBuf, String)> {
    fn walk(d: &Path, depth: u8, out: &mut Vec<PathBuf>) {
        if d.join("_meta.lua").is_file() && d.join("main.lua").is_file() { out.push(d.to_path_buf()); return; }
        if depth == 0 { return; }
        let Ok(rd) = std::fs::read_dir(d) else { return };
        // file_type doesn't follow symlinks, so a link to . or / can't loop or escape
        let mut subs: Vec<PathBuf> = rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect();
        subs.sort();
        for p in subs { walk(&p, depth - 1, out); }
    }
    let mut found = vec![];
    walk(root, 3, &mut found);
    // prefer a folder that is already named *.koplugin
    found.sort_by_key(|p| !p.to_string_lossy().ends_with(".koplugin"));
    let dir = found.into_iter().next().ok_or_else(|| tr(
        "The download has no folder with both _meta.lua and main.lua, so it isn't a KOReader plugin.",
        "La descarga no tiene una carpeta con _meta.lua y main.lua, así que no es un plugin de KOReader."))?;
    let own = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let name = if own.ends_with(".koplugin") { own } else if repo.ends_with(".koplugin") { repo.into() } else { format!("{repo}.koplugin") };
    safe_name(&name)?;
    Ok((dir, name))
}

/// `version = "1.2"` or `version = 1.2` from a _meta.lua, which lua::parse can't read (it calls functions).
pub fn meta_version(src: &str) -> String {
    src.lines().find_map(|l| {
        let r = l.trim().strip_prefix("version")?.trim_start().strip_prefix('=')?.trim();
        let v = r.trim_end_matches(',').trim().trim_matches(|c| c == '"' || c == '\'');
        (!v.is_empty()).then(|| v.to_string())
    }).unwrap_or_default()
}

/// A single path component that is safe in a remote shell path.
pub fn safe_name(n: &str) -> Res<()> {
    if n.is_empty() || n == "." || n == ".." || n.contains('/') || n.chars().any(char::is_control) {
        return Err(tr(format!("Unsafe name: {n:?}"), format!("Nombre inseguro: {n:?}")));
    }
    Ok(())
}

fn tar_dir(dir: &Path) -> Res<Vec<u8>> {
    // ustar for BusyBox tar; COPYFILE_DISABLE keeps macOS from adding ._ files
    let o = std::process::Command::new("tar").env("COPYFILE_DISABLE", "1")
        .args(["--format=ustar", "-cf", "-", "-C"]).arg(dir).arg(".")
        .output().map_err(|e| e.to_string())?;
    if !o.status.success() { return Err(String::from_utf8_lossy(&o.stderr).trim().to_string()); }
    Ok(o.stdout)
}

// ---------- what is installed ----------

#[derive(Serialize, Debug, PartialEq)]
pub struct Owned { kind: &'static str, id: String, version: String, update: bool, removable: bool }

pub struct Remote { pub registry: Res<Value>, pub plugins: Vec<String>, pub patches: Vec<String>, pub fonts: Vec<String> }

pub const LIST_CMD: &str = "R=/mnt/us/koreader; cat $R/settings/Storefront_installs.lua 2>/dev/null; echo; echo @@KOB@@; \
     ls $R/plugins 2>/dev/null; echo @@KOB@@; ls $R/patches 2>/dev/null; echo @@KOB@@; ls $R/fonts 2>/dev/null";

pub fn parse_list(out: &str) -> Remote {
    let mut parts = out.split(SEP);
    let reg = parts.next().unwrap_or("");
    let mut names = || parts.next().unwrap_or("").lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect::<Vec<_>>();
    Remote { registry: read_registry(reg), plugins: names(), patches: names(), fonts: names() }
}

/// Install state of every catalog item that is on the Kindle.
pub fn owned(c: &Value, r: &Remote) -> Vec<Owned> {
    let empty = json!({ "plugins": {}, "patches": {}, "fonts": {} });
    let reg = r.registry.as_ref().unwrap_or(&empty);
    let mut v = vec![];
    for p in c["plugins"].as_array().into_iter().flatten() {
        let id = s(p, "full_name");
        let rec = reg["plugins"].as_object().and_then(|m| m.values().find(|x| s(x, "repo_full_name") == id));
        match rec {
            Some(rec) if r.plugins.iter().any(|d| d == s(rec, "dirname")) => {
                let tag = s(rec, "installed_tag");
                let update = match p["latest_release"]["tag_name"].as_str() {
                    Some(t) if !t.is_empty() => t != tag,
                    _ => iso_secs(s(p, "pushed_at")).zip(rec["matched_at"].as_i64()).is_some_and(|(pushed, at)| pushed > at),
                };
                let version = if tag.is_empty() { s(rec, "installed_version") } else { tag };
                v.push(Owned { kind: "plugin", id: id.into(), version: version.into(), update, removable: true });
            }
            _ => {
                // installed by hand or before Storefront: the folder carries the repo name
                let name = s(p, "name");
                let dir = if name.ends_with(".koplugin") { name.to_string() } else { format!("{name}.koplugin") };
                if r.plugins.contains(&dir) {
                    v.push(Owned { kind: "plugin", id: id.into(), version: String::new(), update: false, removable: false });
                }
            }
        }
    }
    for (p, f) in c["patches"].as_array().into_iter().flatten().flat_map(|p| p["patch_files"].as_array().into_iter().flatten().map(move |f| (p, f))) {
        let id = format!("{}/{}", s(p, "full_name"), s(f, "path"));
        let rec = reg["patches"].as_object().and_then(|m| m.values().find(|x| s(x, "repo_full_name") == s(p, "full_name") && s(x, "path") == s(f, "path")));
        let on_disk = |n: &str| r.patches.iter().any(|x| x == n);
        match rec {
            Some(rec) if on_disk(s(rec, "filename")) => {
                let sha = s(rec, "sha");
                v.push(Owned { kind: "patch", id, version: sha.chars().take(7).collect(), update: sha != s(f, "sha"), removable: true });
            }
            _ if on_disk(s(f, "filename")) => v.push(Owned { kind: "patch", id, version: String::new(), update: false, removable: false }),
            _ => {}
        }
    }
    for f in c["fonts"].as_array().into_iter().flatten() {
        let name = s(f, "name");
        let rec = reg["fonts"].get(name.to_lowercase());
        if let Some(rec) = rec {
            // a font Storefront matched in /mnt/us/fonts has no folder of its own to remove
            let removable = r.fonts.iter().any(|d| d == s(rec, "font_name"));
            v.push(Owned { kind: "font", id: name.into(), version: s(rec, "version").into(), update: false, removable });
        } else if r.fonts.iter().any(|d| d == name) {
            v.push(Owned { kind: "font", id: name.into(), version: String::new(), update: false, removable: false });
        }
    }
    v
}

// ---------- install / uninstall ----------

#[derive(Serialize, Clone)]
pub struct Step { pub step: &'static str, pub done: u64, pub total: u64 }

pub async fn remote_registry() -> Res<Value> {
    read_registry(&ssh::run(&format!("cat {} 2>/dev/null; true", sq(REGISTRY))).await?)
}

pub async fn save_registry(reg: &Value) -> Res<()> {
    let p = sq(REGISTRY);
    ssh::run_with_input(&format!("cat > {p}.kob-new && mv {p}.kob-new {p}"), write_registry(reg).as_bytes()).await
}

/// Sends `dir`'s contents to `parent/name`, swapping folders so the old one survives any failure.
async fn send_dir(dir: &Path, parent: &str, name: &str) -> Res<()> {
    safe_name(name)?;
    let tar = tar_dir(dir)?;
    let (p, n) = (sq(parent), sq(name));
    let new = sq(&format!("{name}.kob-new"));
    let old = sq(&format!("{name}.kob-old"));
    ssh::run_with_input(&format!(
        "mkdir -p {p} && cd {p} && rm -rf {new} {old} && mkdir {new} && tar -xf - -C {new} && \
         {{ [ ! -e {n} ] || mv {n} {old}; }} && \
         if mv {new} {n}; then rm -rf {old}; else [ -e {old} ] && mv {old} {n}; exit 1; fi"), &tar).await
}

async fn send_file(bytes: &[u8], dir: &str, name: &str) -> Res<()> {
    safe_name(name)?;
    let t = sq(&format!("{dir}/{name}"));
    ssh::run_with_input(&format!("cat > {t}.kob-part && mv {t}.kob-part {t}"), bytes).await
}

async fn fetch(url: &str, work: &Path, out: &str, on: &mut impl FnMut(Step)) -> Res<PathBuf> {
    crate::aria::download(url, work, out, |d, t| on(Step { step: "download", done: d, total: t })).await
}

/// Downloads, unpacks and installs one item, then records it the way Storefront does.
/// The caller has already checked KOReader is closed and made the backup.
pub async fn install(c: &Value, kind: &str, id: &str, work: &Path, mut on: impl FnMut(Step)) -> Res<()> {
    let _ = std::fs::remove_dir_all(work);
    let mut reg = remote_registry().await;
    let at = now();
    match kind {
        "plugin" => {
            let p = find_plugin_entry(c, id).ok_or_else(|| tr(format!("{id} isn't in the catalog"), format!("{id} no está en el catálogo")))?;
            let zip = fetch(&plugin_zip_url(p), work, "plugin.zip", &mut on).await?;
            let x = work.join("x");
            unzip(&zip, &x)?;
            let (dir, name) = find_plugin(&x, s(p, "name"))?;
            on(Step { step: "send", done: 0, total: 0 });
            send_dir(&dir, &format!("{KO}/plugins"), &name).await?;
            // the new version ships under another folder name: drop the old folder, or KOReader loads both
            let old = reg.as_ref().ok().and_then(|r| r["plugins"].as_object()?.values().find(|x| s(x, "repo_full_name") == id).map(|x| s(x, "dirname").to_string()));
            if let Some(old) = old.filter(|o| *o != name && safe_name(o).is_ok() && o.ends_with(".koplugin")) {
                ssh::run(&format!("rm -rf {}", sq(&format!("{KO}/plugins/{old}")))).await?;
            }
            let version = meta_version(&std::fs::read_to_string(dir.join("_meta.lua")).unwrap_or_default());
            if let Ok(reg) = reg.as_mut() {
                let tag = p["latest_release"]["tag_name"].as_str().filter(|t| !t.is_empty());
                table(reg, "plugins").retain(|_, r| s(r, "repo_full_name") != id);
                table(reg, "plugins").insert(name.clone(), json!({
                    "dirname": name, "meta_path": format!("{name}/_meta.lua"),
                    "installed_version": version, "installed_tag": tag, "matched_at": at,
                    "plugin_name": name.trim_end_matches(".koplugin"), "repo_full_name": id, "repo_id": p["repo_id"].clone(),
                    "repo_description": s(p, "description"), "owner": s(p, "owner"),
                    "branch": match s(p, "default_branch") { "" => "main", b => b }, "repo": s(p, "name"),
                }));
            }
        }
        "patch" => {
            let (p, f) = find_patch_entry(c, id).ok_or_else(|| tr(format!("{id} isn't in the catalog"), format!("{id} no está en el catálogo")))?;
            let file = fetch(s(f, "download_url"), work, "patch.lua", &mut on).await?;
            // an update keeps a patch the user disabled disabled
            let prev = reg.as_ref().ok().and_then(|r| r["patches"].as_object()?.iter()
                .find(|(_, x)| s(x, "repo_full_name") == s(p, "full_name") && s(x, "path") == s(f, "path")).map(|(k, _)| k.clone()));
            let name = prev.clone().filter(|n| n.ends_with(".disabled")).unwrap_or_else(|| s(f, "filename").into());
            on(Step { step: "send", done: 0, total: 0 });
            send_file(&std::fs::read(file).map_err(|e| e.to_string())?, &format!("{KO}/patches"), &name).await?;
            if let Ok(reg) = reg.as_mut() {
                if let Some(k) = prev { table(reg, "patches").remove(&k); }
                table(reg, "patches").insert(name.clone(), json!({
                    "filename": name, "repo_id": p["repo_id"].clone(), "repo": s(p, "name"), "owner": s(p, "owner"),
                    "sha": s(f, "sha"), "download_url": s(f, "download_url"), "repo_full_name": s(p, "full_name"),
                    "branch": s(f, "branch"), "path": s(f, "path"), "matched_at": at, "repo_description": s(p, "description"),
                }));
            }
        }
        "font" => {
            let f = find_font_entry(c, id).ok_or_else(|| tr(format!("{id} isn't in the catalog"), format!("{id} no está en el catálogo")))?;
            let name = s(f, "name").to_string();
            safe_name(&name)?;
            let zip = fetch(s(f, "download_url"), work, "font.zip", &mut on).await?;
            let x = work.join("x");
            unzip(&zip, &x)?;
            // only the font files, flat, like Storefront's font folder
            let flat = work.join("flat");
            std::fs::create_dir_all(&flat).map_err(|e| e.to_string())?;
            let mut stack = vec![x];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).map_err(|e| e.to_string())?.flatten() {
                    let (p, ft) = (e.path(), e.file_type().map_err(|e| e.to_string())?);
                    if ft.is_dir() { stack.push(p); }
                    else if ft.is_file() && p.extension().is_some_and(|x| ["ttf", "otf"].contains(&x.to_ascii_lowercase().to_str().unwrap_or(""))) {
                        std::fs::copy(&p, flat.join(e.file_name())).map_err(|e| e.to_string())?;
                    }
                }
            }
            if std::fs::read_dir(&flat).map(|mut r| r.next().is_none()).unwrap_or(true) {
                return Err(tr("The zip has no .ttf or .otf files.", "El zip no tiene archivos .ttf ni .otf."));
            }
            on(Step { step: "send", done: 0, total: 0 });
            send_dir(&flat, &format!("{KO}/fonts"), &name).await?;
            if let Ok(reg) = reg.as_mut() {
                table(reg, "fonts").insert(name.to_lowercase(), json!({
                    "is_storefront": true, "font_name": name, "repo": name, "full_name": s(f, "full_name"), "owner": s(f, "owner"),
                    "download_url": s(f, "download_url"), "version": "1.0", "installed_at": at, "matched_at": at, "full_installed": true,
                }));
            }
        }
        _ => return Err(format!("unknown kind: {kind}")),
    }
    let _ = std::fs::remove_dir_all(work);
    match reg {
        Ok(reg) => save_registry(&reg).await,
        Err(err) => Err(tr(
            format!("Installed, but Storefront_installs.lua couldn't be read ({err}), so Storefront won't list it as installed."),
            format!("Quedó instalado, pero no pude leer Storefront_installs.lua ({err}), así que Storefront no lo va a mostrar como instalado."))),
    }
}

/// Paths under /mnt/us that an install or removal of this item replaces, for the backup before it.
pub fn targets(c: &Value, kind: &str, id: &str, reg: &Value) -> Vec<String> {
    let (dir, mut names): (&str, Vec<String>) = match kind {
        "plugin" => {
            let mut d: Vec<String> = reg["plugins"].as_object().into_iter().flat_map(|m| m.values())
                .filter(|r| s(r, "repo_full_name") == id).map(|r| s(r, "dirname").to_string()).collect();
            if let Some(p) = find_plugin_entry(c, id) {
                let n = s(p, "name");
                d.push(if n.ends_with(".koplugin") { n.into() } else { format!("{n}.koplugin") });
            }
            ("plugins", d)
        }
        // patches are already in every backup
        "font" => ("fonts", vec![id.to_string()]),
        _ => return vec![],
    };
    names.retain(|n| safe_name(n).is_ok());
    names.dedup();
    names.into_iter().map(|n| format!("koreader/{dir}/{n}")).collect()
}

/// Removes an item the registry knows about, then its registry entry.
pub async fn uninstall(kind: &str, id: &str) -> Res<()> {
    let mut reg = remote_registry().await?;
    let not_ours = || tr(
        format!("{id} wasn't installed from a store, so remove it on the Kindle or in Fonts."),
        format!("{id} no se instaló desde una tienda, así que borralo en la Kindle o en Fuentes."));
    let (table_name, key, path) = match kind {
        "plugin" => {
            let (k, r) = reg["plugins"].as_object().and_then(|m| m.iter().find(|(_, r)| s(r, "repo_full_name") == id)).ok_or_else(not_ours)?;
            let d = s(r, "dirname");
            safe_name(d)?;
            if !d.ends_with(".koplugin") { return Err(tr(format!("Unsafe name: {d:?}"), format!("Nombre inseguro: {d:?}"))); }
            ("plugins", k.clone(), format!("{KO}/plugins/{d}"))
        }
        "patch" => {
            let (k, r) = reg["patches"].as_object().and_then(|m| m.iter().find(|(_, r)| id == format!("{}/{}", s(r, "repo_full_name"), s(r, "path")))).ok_or_else(not_ours)?;
            safe_name(s(r, "filename"))?;
            ("patches", k.clone(), format!("{KO}/patches/{}", s(r, "filename")))
        }
        "font" => {
            let k = id.to_lowercase();
            let r = reg["fonts"].get(&k).ok_or_else(not_ours)?;
            safe_name(s(r, "font_name"))?;
            ("fonts", k, format!("{KO}/fonts/{}", s(r, "font_name")))
        }
        _ => return Err(format!("unknown kind: {kind}")),
    };
    if kind == "font" && ssh::run(&format!("[ -d {} ] && echo y || true", sq(&path))).await?.trim() != "y" {
        return Err(not_ours());
    }
    ssh::run(&format!("rm -rf {}", sq(&path))).await?;
    table(&mut reg, table_name).remove(&key);
    save_registry(&reg).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("fixtures/Storefront_installs.lua");

    #[test]
    fn reads_and_rewrites_the_registry() {
        let v = read_registry(FIXTURE).unwrap();
        assert_eq!(v["plugins"].as_object().unwrap().len(), 9);
        assert_eq!(v["patches"].as_object().unwrap().len(), 3);
        assert_eq!(v["fonts"].as_object().unwrap().len(), 1);
        assert!(v["item_options"].is_object(), "[] becomes {{}}");
        assert_eq!(v["plugins"]["wikireader.koplugin"]["installed_tag"], "v1.0.2");
        assert_eq!(v["patches"]["2-new-status-icons.lua"]["download_url"],
                   "https://raw.githubusercontent.com/SeriousHornet/KOReader.patches/main/2-new-status-icons.lua");
        let again = read_registry(&write_registry(&v)).unwrap();
        assert_eq!(again, v);
        assert_eq!(read_registry("").unwrap(), json!({ "plugins": {}, "patches": {}, "fonts": {}, "item_options": {} }));
        assert!(read_registry("return { [\"installs\"] = \"{oops\" }").is_err());
    }

    #[test]
    fn quotes_like_lua() {
        assert_eq!(lua_quote(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(lua_quote("x\ny\0"), "\"x\\\ny\\000\"");
        let back = lua::parse(&format!("return {{ [\"k\"] = {} }}", lua_quote("q\"\\\n\r end"))).unwrap();
        assert_eq!(back.str("k"), Some("q\"\\\n\r end"));
    }

    #[test]
    fn picks_the_zip() {
        let rel = json!({ "full_name": "a/b.koplugin", "default_branch": "dev", "latest_release": { "tag_name": "v1", "download_url": "https://x/r.zip" } });
        assert_eq!(plugin_zip_url(&rel), "https://x/r.zip");
        let branch = json!({ "full_name": "a/b.koplugin", "default_branch": "dev", "latest_release": null });
        assert_eq!(plugin_zip_url(&branch), "https://github.com/a/b.koplugin/archive/refs/heads/dev.zip");
    }

    fn tree(files: &[&str]) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!("kob-store-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&root);
        for f in files {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "-- x").unwrap();
        }
        root
    }

    #[test]
    fn finds_the_plugin_folder() {
        let r = tree(&["b-main/x.koplugin/_meta.lua", "b-main/x.koplugin/main.lua", "b-main/README.md"]);
        let (d, n) = find_plugin(&r, "b").unwrap();
        assert!(d.ends_with("b-main/x.koplugin") && n == "x.koplugin");
        let r = tree(&["wiki.koplugin-main/_meta.lua", "wiki.koplugin-main/main.lua"]);
        assert_eq!(find_plugin(&r, "wiki.koplugin").unwrap().1, "wiki.koplugin");
        let r = tree(&["clock-1.2/_meta.lua", "clock-1.2/main.lua"]);
        assert_eq!(find_plugin(&r, "clock").unwrap().1, "clock.koplugin");
        let r = tree(&["b-main/x.koplugin/_meta.lua"]);
        assert!(find_plugin(&r, "b").is_err(), "no main.lua");
    }

    #[test]
    fn unzip_rejects_escaping_paths() {
        use std::io::Write;
        let dir = tree(&[]);
        std::fs::create_dir_all(&dir).unwrap();
        let z = dir.join("bad.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&z).unwrap());
        w.start_file("../evil.lua", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();
        assert!(unzip(&z, &dir.join("out")).is_err());
        assert!(!dir.join("evil.lua").exists());
    }

    #[test]
    fn checks_names() {
        for bad in ["", ".", "..", "a/b", "x\n"] { assert!(safe_name(bad).is_err(), "{bad:?}"); }
        for ok in ["wikireader.koplugin", "2-x.lua.disabled", "Atkinson Hyperlegible", "it's"] { assert!(safe_name(ok).is_ok(), "{ok}"); }
    }

    #[test]
    fn reads_meta_version() {
        assert_eq!(meta_version("local _ = require(\"gettext\")\nreturn {\n    name = \"x\",\n    version = \"1.0.2\",\n}"), "1.0.2");
        assert_eq!(meta_version("return { version = 3, }"), "");
        assert_eq!(meta_version("return {\n version = 3,\n}"), "3");
        assert_eq!(meta_version("return { name = 'x' }"), "");
    }

    #[test]
    fn converts_iso_dates() {
        assert_eq!(iso_secs("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_secs("2026-10-13T12:00:00Z"), Some(1791892800));
        assert_eq!(iso_secs("nope"), None);
    }

    #[test]
    fn works_out_what_is_installed() {
        let c = json!({
            "plugins": [
                { "full_name": "chandujr/wikireader.koplugin", "name": "wikireader.koplugin", "latest_release": { "tag_name": "v1.0.3" } },
                { "full_name": "a/clock", "name": "clock", "latest_release": null, "pushed_at": "2030-01-01T00:00:00Z" },
                { "full_name": "a/none", "name": "none", "latest_release": null },
            ],
            "patches": [{ "full_name": "SeriousHornet/KOReader.patches", "patch_files": [
                { "path": "2-new-status-icons.lua", "filename": "2-new-status-icons.lua", "sha": "1f9268440797163e7f423a1dd96b2014a05533a4" },
            ]}],
            "fonts": [{ "name": "Atkinson Hyperlegible" }],
        });
        let remote = Remote {
            registry: read_registry(FIXTURE),
            plugins: vec!["wikireader.koplugin".into(), "clock.koplugin".into()],
            patches: vec!["2-new-status-icons.lua".into()],
            fonts: vec![],
        };
        let o = owned(&c, &remote);
        assert_eq!(o[0], Owned { kind: "plugin", id: "chandujr/wikireader.koplugin".into(), version: "v1.0.2".into(), update: true, removable: true });
        assert_eq!(o[1], Owned { kind: "plugin", id: "a/clock".into(), version: String::new(), update: false, removable: false });
        assert_eq!(o[2], Owned { kind: "patch", id: "SeriousHornet/KOReader.patches/2-new-status-icons.lua".into(), version: "1f92684".into(), update: false, removable: true });
        assert_eq!(o[3], Owned { kind: "font", id: "Atkinson Hyperlegible".into(), version: "1.0".into(), update: false, removable: false });
        assert_eq!(o.len(), 4);
    }

    #[test]
    fn backs_up_what_changes() {
        let c = json!({ "plugins": [{ "full_name": "chandujr/wikireader.koplugin", "name": "wikireader.koplugin" }, { "full_name": "a/clock", "name": "clock" }] });
        let reg = read_registry(FIXTURE).unwrap();
        assert_eq!(targets(&c, "plugin", "chandujr/wikireader.koplugin", &reg), ["koreader/plugins/wikireader.koplugin"]);
        assert_eq!(targets(&c, "plugin", "a/clock", &reg), ["koreader/plugins/clock.koplugin"]);
        assert_eq!(targets(&c, "font", "Inter", &reg), ["koreader/fonts/Inter"]);
        assert!(targets(&c, "font", "../x", &reg).is_empty());
        assert!(targets(&c, "patch", "a/b/c.lua", &reg).is_empty());
        let bad = json!({ "plugins": { "x": { "repo_full_name": "a/b", "dirname": "../settings" } } });
        assert!(targets(&json!({}), "plugin", "a/b", &bad).is_empty());
    }

    #[test]
    fn parses_the_listing() {
        let r = parse_list("return {}\n@@KOB@@\na.koplugin\nb.koplugin\n@@KOB@@\n@@KOB@@\nnoto\n");
        assert_eq!(r.plugins, ["a.koplugin", "b.koplugin"]);
        assert!(r.patches.is_empty());
        assert_eq!(r.fonts, ["noto"]);
        assert!(r.registry.is_ok());
    }

    /// Network, aria2c and the Kindle, without touching KOReader's folders:
    /// `cargo test store_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn store_live() {
        tauri::async_runtime::block_on(async {
            ssh::set_target(ssh::resolve().await.expect("resolve"));
            let work = std::env::temp_dir().join("kob-store-live");
            let _ = std::fs::remove_dir_all(&work);
            // a release asset and an api.github.com zipball
            for (url, repo) in [
                ("https://github.com/tachibana-shin/smartdelete.koplugin/releases/download/v1.0.2/smartdelete.koplugin-1.0.2.zip", "smartdelete.koplugin"),
                ("https://github.com/chandujr/wikireader.koplugin/archive/refs/heads/main.zip", "wikireader.koplugin"),
            ] {
                let zip = crate::aria::download(url, &work, "p.zip", |d, t| println!("  {d}/{t}")).await.expect("download");
                let x = work.join(repo);
                unzip(&zip, &x).expect("unzip");
                let (dir, name) = find_plugin(&x, repo).expect("plugin folder");
                println!("{repo}: {name}, version {:?}", meta_version(&std::fs::read_to_string(dir.join("_meta.lua")).unwrap()));
                // twice, so the second run swaps folders
                send_dir(&dir, "/tmp/kob-store-test", &name).await.expect("send");
                send_dir(&dir, "/tmp/kob-store-test", &name).await.expect("send again");
                let ls = ssh::run(&format!("ls -a /tmp/kob-store-test; ls {}", sq(&format!("/tmp/kob-store-test/{name}")))).await.unwrap();
                println!("{ls}");
                assert!(ls.contains("_meta.lua") && !ls.contains(".kob-old") && !ls.contains(".kob-new") && !ls.contains("._"));
            }
            ssh::run("rm -rf /tmp/kob-store-test").await.unwrap();
            let reg = remote_registry().await.expect("registry");
            assert_eq!(read_registry(&write_registry(&reg)).unwrap(), reg);
            println!("registry: {} plugins", reg["plugins"].as_object().unwrap().len());
        });
    }
}
