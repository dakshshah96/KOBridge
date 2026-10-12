//! Highlights from KOReader sidecars (`*.sdr/metadata.*.lua`) to Markdown notes.

use crate::lua;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MARK: &str = "@@KOB@@ ";

/// One ssh call: every sidecar that has annotations, each prefixed by a marker line.
pub const FETCH_CMD: &str = r#"find /mnt/us/books /mnt/us/documents -name dictionaries -prune -o -path '*.sdr/metadata.*.lua' \
 -exec grep -l '\["annotations"\] = {$' {} + 2>/dev/null | while IFS= read -r f; do printf '@@KOB@@ %s\n' "$f"; cat "$f"; echo; done"#;

#[derive(Serialize, Deserialize, Clone)]
pub struct Note {
    pub text: String,
    pub note: Option<String>,
    pub chapter: Option<String>,
    pub page: Option<i64>,
    pub datetime: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Book {
    pub sidecar: String,
    pub title: String,
    pub authors: String,
    pub percent: f64,
    pub notes: Vec<Note>,
}

pub fn parse_dump(dump: &str) -> (Vec<Book>, Vec<String>) {
    let (mut books, mut errors) = (Vec::new(), Vec::new());
    for chunk in dump.split(MARK).skip(1) {
        let (path, body) = chunk.split_once('\n').unwrap_or((chunk, ""));
        match lua::parse(body) {
            Ok(v) => if let Some(b) = book(path, &v) { books.push(b) },
            Err(e) => errors.push(format!("{path}: {e}")),
        }
    }
    books.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    (books, errors)
}

fn book(path: &str, v: &lua::Lua) -> Option<Book> {
    let notes: Vec<Note> = v.get("annotations")?.items().into_iter()
        .filter_map(|a| {
            let clean = |s: &str| s.replace('\u{ad}', ""); // EPUB soft hyphens
            let text = clean(a.str("text")?.trim());
            (!text.is_empty()).then(|| Note {
                text,
                note: a.str("note").map(clean).filter(|s| !s.trim().is_empty()),
                chapter: a.str("chapter").map(clean),
                page: a.num("pageno").map(|n| n as i64),
                datetime: a.str("datetime").map(str::to_string),
            })
        })
        .collect();
    if notes.is_empty() { return None; }
    let props = v.get("doc_props");
    let fallback = Path::new(path).parent().and_then(|p| p.file_stem()).and_then(|s| s.to_str()).unwrap_or("Sin título");
    Some(Book {
        sidecar: path.to_string(),
        title: props.and_then(|p| p.str("title")).filter(|s| !s.is_empty()).unwrap_or(fallback).to_string(),
        authors: props.and_then(|p| p.str("authors")).unwrap_or("").replace('\n', ", "),
        percent: v.num("percent_finished").unwrap_or(0.0),
        notes,
    })
}

fn yaml(s: &str) -> String { format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")) }

pub fn markdown(b: &Book) -> String {
    let mut s = format!(
        "---\ntitle: {}\nauthor: {}\nprogress: {}%\nhighlights: {}\nsource: koreader\n---\n\n# {}\n",
        yaml(&b.title), yaml(&b.authors), (b.percent * 100.0).round(), b.notes.len(), b.title);
    if !b.authors.is_empty() { s += &format!("*{}*\n", b.authors); }
    for n in &b.notes {
        s.push('\n');
        for line in n.text.lines() { s += &format!("> {line}\n"); }
        let mut refs = vec![];
        if let Some(c) = &n.chapter { refs.push(c.clone()); }
        if let Some(p) = n.page { refs.push(format!("p. {p}")); }
        if let Some(d) = &n.datetime { refs.push(d.get(..10).unwrap_or(d).to_string()); }
        if !refs.is_empty() { s += &format!(">\n> — {}\n", refs.join(" · ")); }
        if let Some(note) = &n.note { s += &format!("\n**Nota:** {}\n", note.trim()); }
    }
    s
}

/// File name safe for macOS and Obsidian links.
pub fn file_name(title: &str) -> String {
    let clean: String = title.chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']') { ' ' } else { c })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean: String = clean.chars().take(120).collect();
    format!("{}.md", if clean.is_empty() { "Sin título".into() } else { clean })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_to_markdown() {
        let dump = "@@KOB@@ /mnt/us/books/A (1).sdr/metadata.epub.lua\nreturn {\n [\"annotations\"] = {\n  [1] = { [\"text\"] = \"uno\\\ndos\", [\"pageno\"] = 5, [\"chapter\"] = \"Cap\", [\"datetime\"] = \"2026-08-05 06:52:57\", [\"note\"] = \"ojo\" },\n  [2] = { [\"text\"] = \"  \" },\n },\n [\"doc_props\"] = { [\"title\"] = \"Libro: \\\"A\\\"\", [\"authors\"] = \"X\\\nY\" },\n [\"percent_finished\"] = 0.5,\n}\n\n@@KOB@@ /mnt/us/books/B.sdr/metadata.epub.lua\nreturn { [\"annotations\"] = {} }\n";
        let (books, errs) = parse_dump(dump);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(books.len(), 1, "empty annotations are skipped");
        let b = &books[0];
        assert_eq!(b.authors, "X, Y");
        assert_eq!(b.notes.len(), 1, "blank highlight dropped");
        let md = markdown(b);
        assert!(md.contains("title: \"Libro: \\\"A\\\"\""));
        assert!(md.contains("> uno\n> dos\n>\n> — Cap · p. 5 · 2026-08-05\n"));
        assert!(md.contains("**Nota:** ojo"));
        assert_eq!(file_name(&b.title), "Libro A.md");
    }
}
