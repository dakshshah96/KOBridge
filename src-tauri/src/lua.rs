//! Reader for the Lua tables KOReader writes (`return { ["k"] = v, [1] = {...} }`).
//! Only the subset its serializer emits: tables, strings, numbers, booleans, nil.

#[derive(Debug, Clone, PartialEq)]
pub enum Lua {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
    Table(Vec<(Key, Lua)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Int(i64),
    Str(String),
}

impl Lua {
    pub fn get(&self, k: &str) -> Option<&Lua> {
        match self {
            Lua::Table(es) => es.iter().find(|(key, _)| matches!(key, Key::Str(s) if s == k)).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn str(&self, k: &str) -> Option<&str> {
        match self.get(k)? { Lua::Str(s) => Some(s), _ => None }
    }
    pub fn num(&self, k: &str) -> Option<f64> {
        match self.get(k)? { Lua::Num(n) => Some(*n), _ => None }
    }
    /// Array part, in index order.
    pub fn items(&self) -> Vec<&Lua> {
        let Lua::Table(es) = self else { return vec![] };
        let mut v: Vec<(i64, &Lua)> = es.iter().filter_map(|(k, v)| match k { Key::Int(i) => Some((*i, v)), _ => None }).collect();
        v.sort_by_key(|(i, _)| *i);
        v.into_iter().map(|(_, v)| v).collect()
    }
}

pub fn parse(src: &str) -> Result<Lua, String> {
    let mut p = P { b: src.as_bytes(), i: 0 };
    p.ws();
    if p.b[p.i..].starts_with(b"return") { p.i += 6; }
    let v = p.value()?;
    p.ws();
    if p.i < p.b.len() { return Err(p.err("datos sobrantes")); }
    Ok(v)
}

struct P<'a> { b: &'a [u8], i: usize }

impl P<'_> {
    fn err(&self, m: &str) -> String { format!("lua: {m} en byte {}", self.i) }
    fn peek(&self) -> Option<u8> { self.b.get(self.i).copied() }

    fn ws(&mut self) {
        loop {
            while matches!(self.peek(), Some(c) if c.is_ascii_whitespace()) { self.i += 1; }
            if self.b[self.i..].starts_with(b"--") {
                while !matches!(self.peek(), None | Some(b'\n')) { self.i += 1; }
            } else { return; }
        }
    }

    fn eat(&mut self, c: u8) -> Result<(), String> {
        self.ws();
        if self.peek() == Some(c) { self.i += 1; Ok(()) } else { Err(self.err(&format!("se esperaba '{}'", c as char))) }
    }

    fn value(&mut self) -> Result<Lua, String> {
        self.ws();
        match self.peek().ok_or_else(|| self.err("fin inesperado"))? {
            b'{' => self.table(),
            b'"' | b'\'' => Ok(Lua::Str(self.string()?)),
            c if c == b'-' || c == b'.' || c.is_ascii_digit() => self.number(),
            _ => {
                let w = self.ident();
                match w.as_str() {
                    "true" => Ok(Lua::Bool(true)),
                    "false" => Ok(Lua::Bool(false)),
                    "nil" => Ok(Lua::Nil),
                    _ => Err(self.err("valor desconocido")),
                }
            }
        }
    }

    fn ident(&mut self) -> String {
        let s = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_') { self.i += 1; }
        String::from_utf8_lossy(&self.b[s..self.i]).into_owned()
    }

    fn number(&mut self) -> Result<Lua, String> {
        let s = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'+' | b'.')) { self.i += 1; }
        let t = std::str::from_utf8(&self.b[s..self.i]).unwrap();
        t.parse::<f64>().map(Lua::Num).map_err(|_| self.err("número inválido"))
    }

    fn string(&mut self) -> Result<String, String> {
        let q = self.b[self.i];
        self.i += 1;
        let mut out = Vec::new();
        loop {
            let c = self.peek().ok_or_else(|| self.err("texto sin cerrar"))?;
            self.i += 1;
            if c == q { break; }
            if c != b'\\' { out.push(c); continue; }
            let e = self.peek().ok_or_else(|| self.err("escape sin cerrar"))?;
            self.i += 1;
            match e {
                b'n' | b'\n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'a' => out.push(7),
                b'b' => out.push(8),
                b'f' => out.push(12),
                b'v' => out.push(11),
                b'z' => while matches!(self.peek(), Some(c) if c.is_ascii_whitespace()) { self.i += 1; },
                b'x' => {
                    let h = std::str::from_utf8(self.b.get(self.i..self.i + 2).ok_or_else(|| self.err("\\x corto"))?).unwrap_or("");
                    out.push(u8::from_str_radix(h, 16).map_err(|_| self.err("\\x inválido"))?);
                    self.i += 2;
                }
                d if d.is_ascii_digit() => {
                    let mut n = (d - b'0') as u32;
                    for _ in 0..2 {
                        match self.peek() { Some(c) if c.is_ascii_digit() => { n = n * 10 + (c - b'0') as u32; self.i += 1; } _ => break }
                    }
                    out.push(n as u8);
                }
                other => out.push(other), // \\ \" \'
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn table(&mut self) -> Result<Lua, String> {
        self.eat(b'{')?;
        let (mut es, mut next) = (Vec::new(), 1i64);
        loop {
            self.ws();
            match self.peek() {
                Some(b'}') => { self.i += 1; return Ok(Lua::Table(es)); }
                Some(b'[') => {
                    self.i += 1;
                    let k = match self.value()? {
                        Lua::Str(s) => Key::Str(s),
                        Lua::Num(n) if n.fract() == 0.0 => Key::Int(n as i64),
                        _ => return Err(self.err("clave no soportada")),
                    };
                    self.eat(b']')?;
                    self.eat(b'=')?;
                    es.push((k, self.value()?));
                }
                Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                    let save = self.i;
                    let w = self.ident();
                    self.ws();
                    if self.peek() == Some(b'=') {
                        self.i += 1;
                        es.push((Key::Str(w), self.value()?));
                    } else {
                        self.i = save;
                        es.push((Key::Int(next), self.value()?));
                        next += 1;
                    }
                }
                None => return Err(self.err("tabla sin cerrar")),
                _ => { es.push((Key::Int(next), self.value()?)); next += 1; }
            }
            self.ws();
            if matches!(self.peek(), Some(b',' | b';')) { self.i += 1; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a real sidecar: Karl Barth (Tietz), metadata.epub.lua
    const SIDECAR: &str = r#"-- we can read Lua syntax here!
return {
    ["annotations"] = {
        [1] = {
            ["chapter"] = "“Without Windows to the Kingdom of Heaven”: The Tambach Lecture",
            ["datetime"] = "2026-08-05 06:52:57",
            ["pageno"] = 369,
            ["text"] = "we have the freedom of being either naively with God or critically with God.",
        },
        [2] = {
            ["chapter"] = "The New Version of the Epistle to the Romans",
            ["note"] = "ver \"Römerbrief\"",
            ["pageno"] = 481,
            ["text"] = "Calvin too had initially worked out “what is said,”\
Barth then formulated the famous statement.",
        },
    },
    ["doc_props"] = {
        ["authors"] = "Christiane Tietz",
        ["title"] = "Karl Barth",
    },
    ["percent_finished"] = 0.91428571428571,
    ["summary"] = {
        ["status"] = "reading",
    },
    ["empty"] = {},
    ["flag"] = false,
}
"#;

    #[test]
    fn parses_sidecar() {
        let v = parse(SIDECAR).unwrap();
        let ann = v.get("annotations").unwrap().items();
        assert_eq!(ann.len(), 2);
        assert_eq!(ann[0].num("pageno"), Some(369.0));
        assert!(ann[0].str("chapter").unwrap().starts_with("“Without"));
        assert_eq!(ann[1].str("text").unwrap(), "Calvin too had initially worked out “what is said,”\nBarth then formulated the famous statement.");
        assert_eq!(ann[1].str("note"), Some("ver \"Römerbrief\""));
        assert_eq!(v.get("doc_props").unwrap().str("title"), Some("Karl Barth"));
        assert!((v.num("percent_finished").unwrap() - 0.914).abs() < 0.001);
        assert_eq!(v.get("empty").unwrap().items().len(), 0);
        assert_eq!(v.get("flag"), Some(&Lua::Bool(false)));
    }

    #[test]
    fn decimal_escapes_and_arrays() {
        let v = parse(r#"{ "a\65\066", 2, -1.5e2, x = 'y' }"#).unwrap();
        let it = v.items();
        assert_eq!(it[0], &Lua::Str("aAB".into()));
        assert_eq!(it[2], &Lua::Num(-150.0));
        assert_eq!(v.str("x"), Some("y"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("return { [1] = }").is_err());
        assert!(parse("{ \"open").is_err());
    }
}
