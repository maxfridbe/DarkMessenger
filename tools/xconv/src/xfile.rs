//! Minimal parser for DirectX text `.x` files (`xof 0302txt` / `0303txt`).
//!
//! The file is parsed into a generic tree of data objects. Separators (`;`
//! and `,`) are dropped, so every object carries a flat list of numbers and
//! strings that the interpreters in `model.rs` walk in template order.

use std::fmt;

#[derive(Debug, Clone)]
pub enum Val {
    Num(f64),
    Str(String),
}

#[derive(Debug, Clone)]
pub enum Item {
    Obj(XObj),
    /// `{ name }` reference to an object declared elsewhere.
    Ref(String),
}

#[derive(Debug, Clone, Default)]
pub struct XObj {
    pub kind: String,
    pub name: Option<String>,
    pub data: Vec<Val>,
    pub items: Vec<Item>,
}

impl XObj {
    pub fn children(&self) -> impl Iterator<Item = &XObj> {
        self.items.iter().filter_map(|i| match i {
            Item::Obj(o) => Some(o),
            Item::Ref(_) => None,
        })
    }

    pub fn child(&self, kind: &str) -> Option<&XObj> {
        self.children().find(|c| c.kind == kind)
    }

    pub fn reader(&self) -> Reader<'_> {
        Reader { vals: &self.data, pos: 0, kind: &self.kind }
    }
}

/// Sequential reader over an object's flattened values.
pub struct Reader<'a> {
    vals: &'a [Val],
    pos: usize,
    kind: &'a str,
}

impl Reader<'_> {
    pub fn num(&mut self) -> Result<f64, Error> {
        match self.vals.get(self.pos) {
            Some(Val::Num(n)) => {
                self.pos += 1;
                Ok(*n)
            }
            other => Err(Error(format!("{}: expected number at value {}, got {:?}", self.kind, self.pos, other))),
        }
    }

    pub fn f32(&mut self) -> Result<f32, Error> {
        Ok(self.num()? as f32)
    }

    pub fn usize(&mut self) -> Result<usize, Error> {
        Ok(self.num()? as usize)
    }

    pub fn string(&mut self) -> Result<String, Error> {
        match self.vals.get(self.pos) {
            Some(Val::Str(s)) => {
                self.pos += 1;
                Ok(s.clone())
            }
            other => Err(Error(format!("{}: expected string at value {}, got {:?}", self.kind, self.pos, other))),
        }
    }

    pub fn floats(&mut self, n: usize) -> Result<Vec<f32>, Error> {
        (0..n).map(|_| self.f32()).collect()
    }
}

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Open,
    Close,
    Ident(String),
    Str(String),
    Num(f64),
    Guid,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, Error> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        match c {
            // Brackets only appear in template declarations, which are skipped.
            b' ' | b'\t' | b'\r' | b'\n' | b';' | b',' | b'[' | b']' => i += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'#' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                out.push(Tok::Open);
                i += 1;
            }
            b'}' => {
                out.push(Tok::Close);
                i += 1;
            }
            b'<' => {
                while i < b.len() && b[i] != b'>' {
                    i += 1;
                }
                i += 1;
                out.push(Tok::Guid);
            }
            b'"' => {
                let start = i + 1;
                i = start;
                while i < b.len() && b[i] != b'"' {
                    i += 1;
                }
                out.push(Tok::Str(src[start..i].to_string()));
                i += 1;
            }
            b'-' | b'+' | b'.' | b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < b.len() && matches!(b[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'-' | b'+') {
                    i += 1;
                }
                let text = &src[start..i];
                let n = text
                    .parse::<f64>()
                    .map_err(|_| Error(format!("bad number '{text}' at byte {start}")))?;
                out.push(Tok::Num(n));
            }
            _ if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'-' | b'.')) {
                    i += 1;
                }
                out.push(Tok::Ident(src[start..i].to_string()));
            }
            _ => return Err(Error(format!("unexpected character '{}' at byte {i}", c as char))),
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expect_open(&mut self) -> Result<(), Error> {
        match self.next() {
            Some(Tok::Open) => Ok(()),
            t => Err(Error(format!("expected '{{', got {t:?}"))),
        }
    }

    /// Parses `Kind [name] { ... }` after `Kind` has been consumed.
    fn object(&mut self, kind: String) -> Result<XObj, Error> {
        let name = match self.peek() {
            Some(Tok::Ident(n)) => {
                let n = n.clone();
                self.pos += 1;
                Some(n)
            }
            _ => None,
        };
        self.expect_open()?;
        let mut obj = XObj { kind, name, ..Default::default() };
        loop {
            match self.next() {
                Some(Tok::Close) => return Ok(obj),
                Some(Tok::Guid) => {}
                Some(Tok::Num(n)) => obj.data.push(Val::Num(n)),
                Some(Tok::Str(s)) => obj.data.push(Val::Str(s)),
                Some(Tok::Ident(k)) => obj.items.push(Item::Obj(self.object(k)?)),
                Some(Tok::Open) => {
                    // Reference: { name } (optionally with a GUID).
                    let mut name = String::new();
                    loop {
                        match self.next() {
                            Some(Tok::Close) => break,
                            Some(Tok::Ident(n)) => name = n,
                            Some(Tok::Guid) => {}
                            t => return Err(Error(format!("bad reference token {t:?}"))),
                        }
                    }
                    obj.items.push(Item::Ref(name));
                }
                None => return Err(Error(format!("unexpected end of file inside {}", obj.kind))),
            }
        }
    }

    fn skip_block(&mut self) -> Result<(), Error> {
        // `template Name { ... }` — skip to the matching close brace.
        while !matches!(self.peek(), Some(Tok::Open)) {
            self.next().ok_or_else(|| Error("unexpected end of file in template".into()))?;
        }
        let mut depth = 0;
        loop {
            match self.next() {
                Some(Tok::Open) => depth += 1,
                Some(Tok::Close) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Some(_) => {}
                None => return Err(Error("unexpected end of file in template".into())),
            }
        }
    }
}

/// Parses a whole text `.x` file into its top-level objects.
pub fn parse(src: &str) -> Result<Vec<XObj>, Error> {
    let header = src.get(..16).unwrap_or("");
    if !header.starts_with("xof ") || !header[8..12].eq_ignore_ascii_case("txt ") {
        return Err(Error(format!("not a text .x file (header {header:?})")));
    }
    let mut p = Parser { toks: tokenize(&src[16..])?, pos: 0 };
    let mut out = Vec::new();
    while let Some(t) = p.next() {
        match t {
            Tok::Ident(k) if k == "template" => p.skip_block()?,
            Tok::Ident(k) => out.push(p.object(k)?),
            t => return Err(Error(format!("unexpected top-level token {t:?}"))),
        }
    }
    Ok(out)
}
