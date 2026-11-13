//! Valve's KeyValues text format (`.vdf`, `.acf`): quoted keys, quoted values or `{ }` blocks.

#[derive(Debug, Clone, PartialEq)]
pub enum Vdf {
    Str(String),
    Obj(Vec<(String, Vdf)>),
}

impl Vdf {
    /// First child named `key` (Valve keys are case-insensitive).
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Obj(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Vdf::Str(s) => Some(s),
            Vdf::Obj(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Obj(items) => items,
            Vdf::Str(_) => &[],
        }
    }
}

/// Parses a document into one root object. Malformed tails are dropped, not fatal: Steam
/// rewrites these files while running and a half-written one shouldn't hide a whole library.
pub fn parse(src: &str) -> Vdf {
    let mut tokens = Tokens { chars: src.chars().peekable() };
    Vdf::Obj(parse_obj(&mut tokens))
}

enum Tok {
    Str(String),
    Open,
    Close,
}

struct Tokens<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl Iterator for Tokens<'_> {
    type Item = Tok;
    fn next(&mut self) -> Option<Tok> {
        loop {
            let c = self.chars.next()?;
            match c {
                '{' => return Some(Tok::Open),
                '}' => return Some(Tok::Close),
                '"' => {
                    let mut s = String::new();
                    while let Some(c) = self.chars.next() {
                        match c {
                            '"' => break,
                            '\\' => match self.chars.next() {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some(o) => s.push(o),
                                None => break,
                            },
                            o => s.push(o),
                        }
                    }
                    return Some(Tok::Str(s));
                }
                '/' if self.chars.peek() == Some(&'/') => {
                    for c in self.chars.by_ref() {
                        if c == '\n' { break; }
                    }
                }
                c if c.is_whitespace() => {}
                // Unquoted token (rare, `#include` style): read to whitespace.
                c => {
                    let mut s = String::from(c);
                    while let Some(&n) = self.chars.peek() {
                        if n.is_whitespace() || n == '{' || n == '}' || n == '"' { break; }
                        s.push(n);
                        self.chars.next();
                    }
                    return Some(Tok::Str(s));
                }
            }
        }
    }
}

fn parse_obj(tokens: &mut Tokens) -> Vec<(String, Vdf)> {
    let mut items = Vec::new();
    while let Some(tok) = tokens.next() {
        let key = match tok {
            Tok::Str(k) => k,
            Tok::Close => break,
            Tok::Open => continue,
        };
        match tokens.next() {
            Some(Tok::Str(v)) => items.push((key, Vdf::Str(v))),
            Some(Tok::Open) => items.push((key, Vdf::Obj(parse_obj(tokens)))),
            Some(Tok::Close) | None => break,
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_blocks_escapes_and_comments() {
        let doc = parse(
            r#"
            // header
            "libraryfolders"
            {
                "0" { "path" "C:\\Program Files (x86)\\Steam" "apps" { "228980" "1" } }
                "1" { "PATH" "D:\\SteamLibrary" }
            }"#,
        );
        let libs = doc.get("libraryfolders").unwrap();
        assert_eq!(libs.entries().len(), 2);
        assert_eq!(libs.get("0").unwrap().str("path"), Some(r"C:\Program Files (x86)\Steam"));
        assert_eq!(libs.get("1").unwrap().str("path"), Some(r"D:\SteamLibrary"), "keys ignore case");
        assert_eq!(libs.get("0").unwrap().get("apps").unwrap().str("228980"), Some("1"));
    }

    #[test]
    fn a_truncated_file_keeps_what_was_read() {
        let doc = parse(r#""AppState" { "appid" "4000" "name" "Garry's Mod" "Installed"#);
        let app = doc.get("AppState").unwrap();
        assert_eq!(app.str("name"), Some("Garry's Mod"));
    }
}
