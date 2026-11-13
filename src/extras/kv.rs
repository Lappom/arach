//! Valve's binary KeyValues (`appcache\stats\*.bin`): a type byte, a NUL-terminated key, then
//! the value; `0x08` closes an object.

#[derive(Debug, Clone, PartialEq)]
pub enum Kv {
    Obj(Vec<(String, Kv)>),
    Str(String),
    Int(i64),
    Float(f32),
}

impl Kv {
    pub fn get(&self, key: &str) -> Option<&Kv> {
        match self {
            Kv::Obj(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn entries(&self) -> &[(String, Kv)] {
        match self {
            Kv::Obj(items) => items,
            _ => &[],
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Kv::Int(n) => Some(*n),
            Kv::Str(s) => s.trim().parse().ok(),
            Kv::Float(f) => Some(*f as i64),
            Kv::Obj(_) => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Kv::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// The whole file as one root object; a truncated file keeps what was read.
pub fn parse(bytes: &[u8]) -> Kv {
    let mut at = 0;
    Kv::Obj(obj(bytes, &mut at, 0))
}

fn obj(b: &[u8], at: &mut usize, depth: usize) -> Vec<(String, Kv)> {
    let mut items = Vec::new();
    while *at < b.len() {
        let kind = b[*at];
        *at += 1;
        if kind == 0x08 || kind == 0x0B { break; }
        let Some(key) = cstr(b, at) else { break };
        let value = match kind {
            0x00 if depth < 32 => Kv::Obj(obj(b, at, depth + 1)),
            0x01 => match cstr(b, at) {
                Some(s) => Kv::Str(s),
                None => break,
            },
            0x02 | 0x04 | 0x06 => match take::<4>(b, at) {
                Some(x) => Kv::Int(i32::from_le_bytes(x) as i64),
                None => break,
            },
            0x03 => match take::<4>(b, at) {
                Some(x) => Kv::Float(f32::from_le_bytes(x)),
                None => break,
            },
            0x07 | 0x0A => match take::<8>(b, at) {
                Some(x) => Kv::Int(i64::from_le_bytes(x)),
                None => break,
            },
            _ => break,
        };
        items.push((key, value));
    }
    items
}

fn cstr(b: &[u8], at: &mut usize) -> Option<String> {
    let end = b[*at..].iter().position(|&c| c == 0)? + *at;
    let s = String::from_utf8_lossy(&b[*at..end]).into_owned();
    *at = end + 1;
    Some(s)
}

fn take<const N: usize>(b: &[u8], at: &mut usize) -> Option<[u8; N]> {
    let x: [u8; N] = b.get(*at..*at + N)?.try_into().ok()?;
    *at += N;
    Some(x)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_user_stats() {
        // What Steam writes for a game with two achievements unlocked (trimmed).
        let mut b = Vec::new();
        b.extend_from_slice(b"\x00cache\x00\x02crc\x00\x01\x02\x03\x04");
        b.extend_from_slice(b"\x001\x00\x02data\x00\x03\x00\x00\x00\x00AchievementTimes\x00\x020\x00\x26\xc3\xc6\x6a\x08\x08\x08\x08");
        let kv = super::parse(&b);
        let stat = kv.get("cache").unwrap().get("1").unwrap();
        assert_eq!(stat.get("data").unwrap().int(), Some(3));
        assert_eq!(stat.get("AchievementTimes").unwrap().get("0").unwrap().int(), Some(0x6ac6c326));
    }
}
