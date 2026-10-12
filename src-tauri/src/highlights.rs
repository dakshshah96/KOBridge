//! Highlights from KOReader sidecars (`*.sdr/metadata.*.lua`); `obsidian.rs` renders them.

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
    /// KOReader's partial md5, the same value as `book.md5` in statistics.sqlite3.
    #[serde(default)]
    pub md5: Option<String>,
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
        md5: v.str("partial_md5_checksum").map(str::to_string),
        notes,
    })
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
    fn parses_dump() {
        let dump = "@@KOB@@ /mnt/us/books/A (1).sdr/metadata.epub.lua\nreturn {\n [\"annotations\"] = {\n  [1] = { [\"text\"] = \"uno\\\ndos\", [\"pageno\"] = 5, [\"chapter\"] = \"Cap\", [\"datetime\"] = \"2026-08-05 06:52:57\", [\"note\"] = \"ojo\" },\n  [2] = { [\"text\"] = \"  \" },\n },\n [\"doc_props\"] = { [\"title\"] = \"Libro: \\\"A\\\"\", [\"authors\"] = \"X\\\nY\" },\n [\"percent_finished\"] = 0.5,\n}\n\n@@KOB@@ /mnt/us/books/B.sdr/metadata.epub.lua\nreturn { [\"annotations\"] = {} }\n";
        let (books, errs) = parse_dump(dump);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(books.len(), 1, "empty annotations are skipped");
        let b = &books[0];
        assert_eq!(b.authors, "X, Y");
        assert_eq!(b.notes.len(), 1, "blank highlight dropped");
        assert_eq!(b.notes[0].text, "uno\ndos");
        assert_eq!(b.notes[0].note.as_deref(), Some("ojo"));
        assert_eq!(file_name(&b.title), "Libro A.md");
    }
}
