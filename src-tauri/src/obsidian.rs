//! Obsidian notes from highlights: render, merge into existing notes without touching manual edits,
//! vault discovery and the `claude -p` enrichment helpers.

use crate::highlights::{Book, Note};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Reading data from the stats snapshot, matched to a book by md5 (or title).
#[derive(Default, Clone)]
pub struct Extra { pub last_read: Option<String>, pub read_time: Option<String> }

pub const MODELS: [&str; 4] = ["haiku", "sonnet", "opus", "fable"];

/// Obsidian block id for a highlight. FNV-1a is stable across Rust versions; DefaultHasher is not.
pub fn block_id(n: &Note) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in n.datetime.as_deref().unwrap_or("").bytes().chain([0]).chain(n.text.bytes()) {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("kob-{:08x}", h as u32)
}

fn yaml(s: &str) -> String { format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")) }

fn chapter(n: &Note) -> &str { n.chapter.as_deref().filter(|c| !c.trim().is_empty()).unwrap_or("Subrayados") }

fn quote(n: &Note) -> String {
    let mut s = String::new();
    for line in n.text.lines() { s += &format!("> {line}\n"); }
    let mut refs = vec![];
    if let Some(c) = &n.chapter { refs.push(c.clone()); }
    if let Some(p) = n.page { refs.push(format!("p. {p}")); }
    if let Some(d) = &n.datetime { refs.push(d.get(..10).unwrap_or(d).to_string()); }
    if !refs.is_empty() { s += &format!(">\n> — {}\n", refs.join(" · ")); }
    if let Some(note) = &n.note {
        s += ">\n";
        for (i, line) in note.trim().lines().enumerate() {
            s += &if i == 0 { format!("> **Nota:** {line}\n") } else { format!("> {line}\n") };
        }
    }
    // Obsidian attaches an id on the line right after a quote to the whole quote
    s + &format!("^{}\n", block_id(n))
}

/// Appends quotes for `notes` not in `have`, opening a `## chapter` heading when the chapter changes.
fn append(out: &mut String, notes: &[Note], have: &mut HashSet<String>, mut last: Option<String>) -> usize {
    let mut added = 0;
    for n in notes {
        if !have.insert(block_id(n)) { continue; }
        let ch = chapter(n);
        if last.as_deref() != Some(ch) {
            *out += &format!("\n## {ch}\n");
            last = Some(ch.to_string());
        }
        *out += "\n";
        *out += &quote(n);
        added += 1;
    }
    added
}

fn ids(doc: &str) -> HashSet<String> {
    doc.lines().filter_map(|l| l.trim().strip_prefix('^')).filter(|s| s.starts_with("kob-")).map(str::to_string).collect()
}

/// Sets `key: value` inside the leading YAML frontmatter, adding the key if missing.
pub fn set_fm(doc: &str, key: &str, value: &str) -> String {
    let Some(rest) = doc.strip_prefix("---\n") else { return doc.to_string() };
    let Some(end) = rest.find("\n---\n").map(|i| i + 1).or_else(|| rest.starts_with("---\n").then_some(0)) else { return doc.to_string() };
    let (fm, tail) = rest.split_at(end);
    let line = format!("{key}: {value}");
    let mut found = false;
    let mut lines: Vec<String> = fm.lines().map(|l| {
        if l.split_once(':').is_some_and(|(k, _)| k == key) { found = true; line.clone() } else { l.to_string() }
    }).collect();
    if !found { lines.push(line); }
    format!("---\n{}\n{tail}", lines.join("\n"))
}

fn owned(doc: String, b: &Book, x: &Extra) -> String {
    let mut doc = set_fm(&doc, "progress", &format!("{}%", (b.percent * 100.0).round()));
    let n = ids(&doc).len().to_string();
    doc = set_fm(&doc, "highlights", &n);
    if let Some(v) = &x.last_read { doc = set_fm(&doc, "last_read", v); }
    if let Some(v) = &x.read_time { doc = set_fm(&doc, "read_time", v); }
    doc
}

/// A new note for `b`.
pub fn render(b: &Book, x: &Extra) -> String {
    let mut s = format!("---\ntitle: {}\nauthor: {}\nsource: koreader\ntags:\n  - koreader\n---\n\n# {}\n",
        yaml(&b.title), yaml(&b.authors), b.title);
    if !b.authors.is_empty() { s += &format!("*{}*\n", b.authors); }
    append(&mut s, &b.notes, &mut HashSet::new(), None);
    owned(s, b, x)
}

pub enum Merge { Unchanged, Updated { doc: String, added: usize }, Legacy }

/// Adds only the highlights whose id is not in `existing` and refreshes the app-owned frontmatter keys.
pub fn merge(existing: &str, b: &Book, x: &Extra) -> Merge {
    let mut have = ids(existing);
    if have.is_empty() { return Merge::Legacy; }
    let last = existing.lines().rev().find_map(|l| l.strip_prefix("## "))
        .filter(|h| !matches!(*h, "Resumen" | "Relacionado")).map(str::to_string);
    let mut doc = existing.to_string();
    if !doc.ends_with('\n') { doc.push('\n'); }
    let added = append(&mut doc, &b.notes, &mut have, last);
    let doc = owned(if added > 0 { doc } else { existing.to_string() }, b, x);
    if doc == existing { Merge::Unchanged } else { Merge::Updated { doc, added } }
}

/// `Libros.md`: one row per book, rewritten on every export.
pub fn index(rows: &[(String, &Book, Extra)]) -> String {
    let cell = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    let mut s = String::from("---\nsource: koreader\n---\n\n# Libros\n\n| Libro | Autor | Progreso | Subrayados | Última lectura |\n|---|---|---|---|---|\n");
    for (stem, b, x) in rows {
        s += &format!("| [[{}]] | {} | {}% | {} | {} |\n", stem, cell(&b.authors), (b.percent * 100.0).round(),
            b.notes.len(), x.last_read.as_deref().unwrap_or(""));
    }
    s
}

/// Vault paths listed in Obsidian's `obsidian.json`.
pub fn vaults(json: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let mut out: Vec<String> = v["vaults"].as_object().into_iter().flatten()
        .filter_map(|(_, x)| x["path"].as_str().map(str::to_string)).collect();
    out.sort();
    out
}

/// The vault containing `dir`: the nearest ancestor with `.obsidian/`, else `dir` itself.
pub fn vault_root(dir: &Path) -> PathBuf {
    dir.ancestors().find(|p| p.join(".obsidian").is_dir()).unwrap_or(dir).to_path_buf()
}

/// Finder-launched apps don't inherit the shell PATH, so look in the usual install dirs first.
pub fn find_claude(home: &Path) -> Option<PathBuf> {
    let known = [PathBuf::from("/opt/homebrew/bin/claude"), PathBuf::from("/usr/local/bin/claude"), home.join(".local/bin/claude")];
    known.into_iter().find(|p| p.is_file()).or_else(|| {
        let out = std::process::Command::new("/bin/zsh").args(["-lc", "command -v claude"]).output().ok()?;
        let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        p.is_file().then_some(p)
    })
}

pub fn prompt(note: &str) -> String {
    format!(r#"Enriquecé la nota de lectura `{note}` de esta bóveda de Obsidian. La carpeta actual es la raíz de la bóveda.

1. Leé la nota. Tiene frontmatter YAML, el título `# ...`, una línea con el autor en cursiva y después secciones `## <capítulo>` con citas.
2. Escribí o reemplazá la sección `## Resumen` entre la línea del autor y la primera sección de capítulo. Poné de 3 a 5 viñetas basadas solo en las citas de la nota.
3. Agregá de 3 a 6 etiquetas temáticas en minúsculas y sin espacios a la lista `tags:` del frontmatter, sin borrar las que ya están.
4. Buscá con Glob y Grep notas de la bóveda que traten los mismos temas. Si encontrás alguna, escribí o reemplazá la sección `## Relacionado` justo después de `## Resumen`, con una viñeta `[[Nombre de la nota]]` por nota y una frase de por qué se relaciona. Enlazá solo notas que existen. Si no hay ninguna, no escribas la sección.
5. No cambies las citas, las líneas que empiezan con `^kob-`, los encabezados de capítulo ni las otras claves del frontmatter.
6. Escribí en el idioma de las citas. Editá solo `{note}`.
"#)
}

pub enum Event { Tool(String), Done, Failed(String) }

/// One line of `--output-format stream-json`.
pub fn event(line: &str) -> Option<Event> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    match v["type"].as_str()? {
        "assistant" => v["message"]["content"].as_array()?.iter()
            .filter(|c| c["type"] == "tool_use")
            .map(|c| {
                let i = &c["input"];
                let file = || i["file_path"].as_str().and_then(|p| Path::new(p).file_name()).map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                let pat = || i["pattern"].as_str().unwrap_or("").to_string();
                Event::Tool(match c["name"].as_str().unwrap_or("") {
                    "Read" => format!("Leyendo {}", file()),
                    "Edit" | "Write" => format!("Editando {}", file()),
                    "Glob" => format!("Buscando notas {}", pat()),
                    "Grep" => format!("Buscando \"{}\"", pat()),
                    other => other.to_string(),
                })
            })
            .last(),
        "result" if v["is_error"].as_bool() == Some(true) || v["subtype"] != "success" =>
            Some(Event::Failed(v["result"].as_str().unwrap_or("Claude terminó con error").to_string())),
        "result" => Some(Event::Done),
        _ => None,
    }
}

/// Percent-encodes a path for an `obsidian://open?path=` URL.
pub fn url_encode(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(text: &str, ch: Option<&str>, dt: &str) -> Note {
        Note { text: text.into(), note: None, chapter: ch.map(Into::into), page: Some(5), datetime: Some(dt.into()) }
    }
    fn book(notes: Vec<Note>) -> Book {
        Book { sidecar: "s".into(), title: "Libro".into(), authors: "Autora".into(), percent: 0.42, md5: None, notes }
    }
    fn x() -> Extra { Extra { last_read: Some("2026-10-11".into()), read_time: Some("5 h 10 min".into()) } }

    #[test]
    fn block_id_is_stable() {
        let n = note("uno", None, "2026-08-05 06:52:57");
        assert_eq!(block_id(&n), block_id(&n.clone()));
        assert_eq!(block_id(&n).len(), 12);
        assert_ne!(block_id(&n), block_id(&note("uno", None, "2026-08-05 06:52:58")));
        assert_eq!(block_id(&n), "kob-0b7f3c79", "must never change between releases");
    }

    #[test]
    fn renders_new_note() {
        let mut n = note("uno\ndos", Some("Cap"), "2026-08-05 06:52:57");
        n.note = Some("ojo".into());
        let md = render(&book(vec![n.clone(), note("tres", None, "2026-08-06 00:00:00")]), &x());
        assert!(md.starts_with("---\ntitle: \"Libro\"\nauthor: \"Autora\"\nsource: koreader\ntags:\n  - koreader\nprogress: 42%\nhighlights: 2\nlast_read: 2026-10-11\nread_time: 5 h 10 min\n---\n"), "{md}");
        assert!(md.contains(&format!("## Cap\n\n> uno\n> dos\n>\n> — Cap · p. 5 · 2026-08-05\n>\n> **Nota:** ojo\n^{}\n", block_id(&n))), "{md}");
        assert!(md.contains("\n## Subrayados\n\n> tres\n"));
    }

    #[test]
    fn merge_appends_only_new_and_keeps_manual_text() {
        let a = note("uno", Some("Cap 1"), "2026-08-05 06:52:57");
        let first = render(&book(vec![a.clone()]), &x());
        assert!(matches!(merge(&first, &book(vec![a.clone()]), &x()), Merge::Unchanged), "idempotent");

        let edited = first.replace("*Autora*\n", "*Autora*\n\n## Resumen\n\n- mío\n").replace("tags:\n  - koreader\n", "tags:\n  - koreader\n  - fe\nmia: 1\n");
        let b = note("dos", Some("Cap 1"), "2026-09-01 10:00:00");
        let c = note("tres", Some("Cap 2"), "2026-09-02 10:00:00");
        let Merge::Updated { doc, added } = merge(&edited, &book(vec![a, b, c]), &x()) else { panic!("expected update") };
        assert_eq!(added, 2);
        assert!(doc.contains("## Resumen\n\n- mío\n") && doc.contains("  - fe\nmia: 1\n"), "{doc}");
        assert!(doc.contains("highlights: 3\n"));
        assert!(doc.ends_with(&format!("^{}\n\n## Cap 2\n\n> tres\n>\n> — Cap 2 · p. 5 · 2026-09-02\n^{}\n", block_id(&note("dos", Some("Cap 1"), "2026-09-01 10:00:00")), block_id(&note("tres", Some("Cap 2"), "2026-09-02 10:00:00")))), "{doc}");
        assert_eq!(doc.matches("## Cap 1").count(), 1, "same chapter: no new heading");
    }

    #[test]
    fn merge_detects_legacy_and_updates_progress() {
        assert!(matches!(merge("---\ntitle: x\n---\n\n> viejo\n", &book(vec![]), &x()), Merge::Legacy));
        let a = note("uno", None, "d");
        let doc = render(&book(vec![a.clone()]), &x());
        let mut b = book(vec![a]);
        b.percent = 0.9;
        let Merge::Updated { doc, added } = merge(&doc, &b, &x()) else { panic!() };
        assert_eq!(added, 0);
        assert!(doc.contains("progress: 90%\n"));
    }

    #[test]
    fn frontmatter_and_misc() {
        assert_eq!(set_fm("---\na: 1\n---\nbody\n", "b", "2"), "---\na: 1\nb: 2\n---\nbody\n");
        assert_eq!(set_fm("---\na: 1\n---\nbody\n", "a", "3"), "---\na: 3\n---\nbody\n");
        assert_eq!(set_fm("sin fm", "a", "1"), "sin fm");
        let j = r#"{"vaults":{"x":{"path":"/b","ts":1},"y":{"path":"/a","open":true}}}"#;
        assert_eq!(vaults(j), ["/a", "/b"]);
        assert!(vaults("no json").is_empty());
        assert_eq!(url_encode("/Users/d/My Vault"), "/Users/d/My%20Vault");
        let b = book(vec![note("uno", None, "d")]);
        assert!(index(&[("Libro".into(), &b, x())]).contains("| [[Libro]] | Autora | 42% | 1 | 2026-10-11 |"));
    }

    #[test]
    fn parses_stream_events() {
        let read = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/v/KOReader/Libro.md"}}]}}"#;
        assert!(matches!(event(read), Some(Event::Tool(t)) if t == "Leyendo Libro.md"));
        assert!(matches!(event(r#"{"type":"result","subtype":"success","is_error":false,"result":"ok"}"#), Some(Event::Done)));
        assert!(matches!(event(r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#), Some(Event::Failed(_))));
        assert!(event(r#"{"type":"system","subtype":"init"}"#).is_none());
        assert!(event("basura").is_none());
    }
}
