//! Anki cards from highlights: the `claude -p` prompt and schema, and the tab-separated file
//! Anki's text importer reads (docs.ankiweb.net/importing/text-files.html).

use crate::highlights::{Book, Note};
use crate::obsidian::block_id;

pub const SCHEMA: &str = r#"{"type":"object","properties":{"cards":{"type":"array","items":{"type":"object","properties":{"source":{"type":"integer"},"front":{"type":"string"},"back":{"type":"string"}},"required":["source","front","back"]}}},"required":["cards"]}"#;

pub fn prompt(b: &Book) -> String {
    let mut quotes = String::new();
    for (i, n) in b.notes.iter().enumerate() {
        quotes += &format!("[{}] {}\n", i + 1, n.text.replace('\n', " "));
        if let Some(note) = &n.note { quotes += &format!("    Nota del lector: {}\n", note.replace('\n', " ")); }
    }
    format!(r#"Estos son los subrayados de "{}"{}. Escribí tarjetas de Anki de pregunta y respuesta para recordar las ideas del libro.

- Una tarjeta por idea que valga recordar. Saltá los subrayados que no tienen una idea propia. Como máximo 2 tarjetas por subrayado.
- La pregunta se entiende sin ver el subrayado y no copia la respuesta. La respuesta tiene una o dos frases.
- `source` es el número del subrayado del que sale la tarjeta.
- Escribí en el idioma de los subrayados. No uses Markdown ni HTML.

{quotes}"#, b.title, if b.authors.is_empty() { String::new() } else { format!(", de {}", b.authors) })
}

pub struct Card { pub guid: String, pub front: String, pub back: String }

/// Reads the `--output-format json` result; each card's guid is the highlight's block id plus its
/// position for that highlight, so importing again updates the same notes in Anki.
pub fn parse(out: &str, notes: &[Note]) -> Result<Vec<Card>, String> {
    let v: serde_json::Value = serde_json::from_str(out.trim()).map_err(|_| "Claude no devolvió JSON".to_string())?;
    if v["is_error"].as_bool() == Some(true) || v["subtype"] != "success" {
        return Err(v["result"].as_str().unwrap_or("Claude terminó con error").to_string());
    }
    let cards = v["structured_output"]["cards"].as_array().ok_or("Claude no devolvió tarjetas")?;
    let mut seen = vec![0usize; notes.len()];
    let mut out = vec![];
    for c in cards {
        let (Some(i), Some(front), Some(back)) = (c["source"].as_u64(), c["front"].as_str(), c["back"].as_str()) else { continue };
        let Some(n) = (i as usize).checked_sub(1).and_then(|i| notes.get(i)) else { continue };
        let k = &mut seen[i as usize - 1];
        *k += 1;
        let mut refs = vec![];
        if let Some(c) = &n.chapter { refs.push(c.clone()); }
        if let Some(p) = n.page { refs.push(format!("p. {p}")); }
        let src = format!("<br><br><i>{}</i>{}", html(&n.text), if refs.is_empty() { String::new() } else { format!("<br><small>{}</small>", html(&refs.join(", "))) });
        out.push(Card { guid: format!("{}-{k}", block_id(n)), front: html(front), back: html(back) + &src });
    }
    Ok(out)
}

fn html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\t', " ").replace('\n', "<br>")
}

/// Deck `KOReader::<title>`; `::` would nest decks, so colons become spaces.
pub fn deck(title: &str) -> String {
    format!("KOReader::{}", title.replace(':', " ").split_whitespace().collect::<Vec<_>>().join(" "))
}

fn tag(title: &str) -> String {
    let t: String = title.chars().map(|c| if c.is_alphanumeric() { c.to_lowercase().next().unwrap() } else { '_' }).collect();
    t.split('_').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("_")
}

pub fn tsv(books: &[(&Book, Vec<Card>)]) -> String {
    let mut s = String::from("#separator:tab\n#html:true\n#columns:guid\tdeck\tFront\tBack\ttags\n#guid column:1\n#deck column:2\n#tags column:5\n");
    for (b, cards) in books {
        for c in cards {
            s += &format!("{}\t{}\t{}\t{}\tkoreader {}\n", c.guid, deck(&b.title), c.front, c.back, tag(&b.title));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Book {
        let n = |t: &str| Note { text: t.into(), note: None, chapter: Some("Cap 1".into()), page: Some(5), datetime: Some("2026-08-05 06:52:57".into()) };
        Book { sidecar: "s".into(), title: "Gracia: un ensayo".into(), authors: "X".into(), percent: 0.5, md5: None, notes: vec![n("uno"), n("a < b\tc")] }
    }

    #[test]
    fn parses_cards_with_stable_guids() {
        let b = book();
        let out = r#"{"type":"result","subtype":"success","is_error":false,"structured_output":{"cards":[
            {"source":1,"front":"¿Q?","back":"R\nS"},{"source":1,"front":"¿Q2?","back":"R2"},
            {"source":2,"front":"¿a<b?","back":"sí"},{"source":9,"front":"x","back":"y"}]}}"#;
        let cards = parse(out, &b.notes).unwrap();
        assert_eq!(cards.len(), 3, "out-of-range source dropped");
        assert_eq!(cards[0].guid, "kob-0b7f3c79-1", "same id as the Obsidian block");
        assert_eq!(cards[1].guid, "kob-0b7f3c79-2");
        assert_eq!(cards[0].back, "R<br>S<br><br><i>uno</i><br><small>Cap 1, p. 5</small>");
        assert_eq!(cards[2].front, "¿a&lt;b?");
        assert!(cards[2].back.contains("a &lt; b c"), "tab stripped: {}", cards[2].back);
        assert!(parse(r#"{"type":"result","subtype":"error_max_turns","is_error":true,"result":"boom"}"#, &b.notes).is_err());
    }

    #[test]
    fn writes_anki_text_file() {
        let b = book();
        let cards = vec![Card { guid: "kob-1-1".into(), front: "F".into(), back: "B".into() }];
        let s = tsv(&[(&b, cards)]);
        assert!(s.starts_with("#separator:tab\n#html:true\n"));
        assert!(s.ends_with("kob-1-1\tKOReader::Gracia un ensayo\tF\tB\tkoreader gracia_un_ensayo\n"), "{s}");
        assert_eq!(s.lines().last().unwrap().split('\t').count(), 5);
    }
}
