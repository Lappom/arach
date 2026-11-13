//! Typing a few letters finds the game: substrings first, then word initials ("gta" → Grand
//! Theft Auto), then letters in order ("kcd" → Kingdom Come: Deliverance).

/// Lowercase ASCII letters, digits and single spaces; accents dropped, symbols gone.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = true;
    for c in s.chars() {
        let c = fold(c);
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            space = false;
        } else if !space && (c.is_whitespace() || matches!(c, '-' | '_' | ':' | '.' | '/' | '&')) {
            out.push(' ');
            space = true;
        }
    }
    if out.ends_with(' ') { out.pop(); }
    out
}

fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'î' | 'ï' | 'í' | 'ì' | 'Î' | 'Ï' => 'i',
        'ô' | 'ö' | 'ó' | 'ò' | 'õ' | 'Ô' | 'Ö' => 'o',
        'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        'ñ' | 'Ñ' => 'n',
        other => other,
    }
}

/// How well `query` (already normalized) matches `name` (already normalized); `None` when
/// it doesn't. Higher is better.
pub fn score(query: &str, name: &str) -> Option<i32> {
    if query.is_empty() { return Some(0); }
    if let Some(at) = name.find(query) {
        let word_start = at == 0 || name.as_bytes()[at - 1] == b' ';
        return Some(1000 - at as i32 + if word_start { 200 } else { 0 } + if at == 0 { 300 } else { 0 });
    }
    let squashed: String = query.chars().filter(|c| *c != ' ').collect();
    let initials: String = name.split(' ').filter_map(|w| w.chars().next()).collect();
    if initials.starts_with(&squashed) {
        return Some(700);
    }
    // Letters in order, rewarding runs of adjacent ones.
    let mut want = squashed.chars().peekable();
    let (mut s, mut last) = (400, usize::MAX - 1);
    for (i, c) in name.chars().enumerate() {
        if want.peek() == Some(&c) {
            want.next();
            s += if i == last + 1 { 6 } else { -2 };
            last = i;
        }
    }
    want.peek().is_none().then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_accents_and_punctuation() {
        assert_eq!(normalize("Kingdom Come: Deliverance II"), "kingdom come deliverance ii");
        assert_eq!(normalize("Pokémon™  Légendes"), "pokemon legendes");
        assert_eq!(normalize("Mount & Blade"), "mount blade");
    }

    #[test]
    fn ranks_prefix_over_inner_over_initials_over_scattered() {
        let n = |s| normalize(s);
        let prefix = score("hade", &n("Hades")).unwrap();
        let inner = score("blade", &n("Mount & Blade II")).unwrap();
        let initials = score("kcd", &n("Kingdom Come: Deliverance II")).unwrap();
        let scattered = score("trra", &n("Terraria")).unwrap();
        assert!(prefix > inner && inner > initials && initials > scattered);
        assert_eq!(score("zzz", &n("Hades")), None);
    }
}
