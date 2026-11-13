//! Steam achievements: read from the client's own cache (`appcache\stats`), where the schema
//! lists every achievement as a bit of a stat and the user's file holds those bits; else from
//! the player's public profile.

use std::path::Path;

use super::kv::{self, Kv};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Achievements {
    pub unlocked: u32,
    pub total: u32,
    /// The last ones unlocked: name and when (Unix seconds, 0 when unknown), newest first.
    pub recent: Vec<(String, i64)>,
    /// Read from the public profile rather than the client's cache.
    pub public: bool,
}

/// Offset between a 64-bit Steam id and the account id in `userdata`.
const ID64_BASE: u64 = 76_561_197_960_265_728;

/// What the Steam client knows locally about `appid`, for the account that played it last.
pub fn local(steam: &Path, appid: u32, french: bool) -> Option<Achievements> {
    let stats = steam.join("appcache/stats");
    let schema = std::fs::read(stats.join(format!("UserGameStatsSchema_{appid}.bin"))).ok()?;
    let schema = kv::parse(&schema);
    let user = account_file(&stats, appid).and_then(|p| std::fs::read(p).ok()).map(|b| kv::parse(&b));
    from_files(&schema, user.as_ref(), appid, french)
}

/// `UserGameStats_<account>_<appid>.bin`, the most recently written when several accounts have one.
fn account_file(stats: &Path, appid: u32) -> Option<std::path::PathBuf> {
    let suffix = format!("_{appid}.bin");
    std::fs::read_dir(stats).ok()?.flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("UserGameStats_") && n.ends_with(&suffix)
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

fn from_files(schema: &Kv, user: Option<&Kv>, appid: u32, french: bool) -> Option<Achievements> {
    let root = schema.get(&appid.to_string()).or_else(|| schema.entries().first().map(|(_, v)| v))?;
    let stats = root.get("stats")?;
    let user_root = user.and_then(|u| u.get("cache").or_else(|| u.entries().first().map(|(_, v)| v)));
    let mut out = Achievements::default();
    let mut unlocked: Vec<(String, i64)> = Vec::new();
    for (stat_id, stat) in stats.entries() {
        let Some(bits) = stat.get("bits") else { continue };
        let ustat = user_root.and_then(|u| u.get(stat_id));
        let data = ustat.and_then(|s| s.get("data")).and_then(Kv::int).unwrap_or(0) as u64;
        let times = ustat.and_then(|s| s.get("AchievementTimes"));
        for (bit, ach) in bits.entries() {
            let Ok(n) = bit.parse::<u32>() else { continue };
            out.total += 1;
            if n < 64 && data & (1 << n) != 0 {
                out.unlocked += 1;
                let name = display_name(ach, french).unwrap_or_else(|| format!("Succès {}", out.unlocked));
                let at = times.and_then(|t| t.get(bit)).and_then(Kv::int).unwrap_or(0);
                unlocked.push((name, at));
            }
        }
    }
    if out.total == 0 { return None; }
    unlocked.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
    out.recent = unlocked.into_iter().take(3).collect();
    Some(out)
}

fn display_name(ach: &Kv, french: bool) -> Option<String> {
    let names = ach.get("display")?.get("name")?;
    if let Some(s) = names.text() { return Some(s.to_string()); }
    let pick = |lang: &str| names.get(lang).and_then(Kv::text).filter(|s| !s.is_empty()).map(String::from);
    (if french { pick("french") } else { None }).or_else(|| pick("english")).or_else(|| names.entries().iter().find_map(|(_, v)| v.text().map(String::from)))
}

/// The accounts signed in on this PC, most recent first, as 64-bit ids.
pub fn accounts(steam: &Path) -> Vec<u64> {
    let Ok(src) = std::fs::read_to_string(steam.join("config/loginusers.vdf")) else {
        // No login list: the folders in userdata are account ids.
        return std::fs::read_dir(steam.join("userdata")).map(|r| {
            r.flatten().filter_map(|e| e.file_name().to_string_lossy().parse::<u64>().ok()).map(|a| a + ID64_BASE).collect()
        }).unwrap_or_default();
    };
    let doc = crate::detect::vdf::parse(&src);
    let mut users: Vec<(bool, i64, u64)> = doc.get("users").map(|u| u.entries().iter().filter_map(|(id, v)| {
        let recent = v.str("MostRecent") == Some("1");
        let ts = v.str("Timestamp").and_then(|t| t.parse().ok()).unwrap_or(0);
        Some((recent, ts, id.parse().ok()?))
    }).collect()).unwrap_or_default();
    users.sort_by(|a, b| b.cmp(a));
    users.into_iter().map(|u| u.2).collect()
}

/// The achievements page of a public profile, as XML: `<achievement closed="1">` for each one won.
pub fn public(agent: &ureq::Agent, id64: u64, appid: u32, french: bool) -> Option<Achievements> {
    let lang = if french { "french" } else { "english" };
    let url = format!("https://steamcommunity.com/profiles/{id64}/stats/{appid}/achievements/?xml=1&l={lang}");
    let body = agent.get(&url).call().ok()?.into_string().ok()?;
    parse_public(&body)
}

fn parse_public(xml: &str) -> Option<Achievements> {
    let mut out = Achievements { public: true, ..Default::default() };
    let mut won: Vec<(String, i64)> = Vec::new();
    for chunk in xml.split("<achievement ").skip(1) {
        out.total += 1;
        if chunk.starts_with("closed=\"1\"") {
            out.unlocked += 1;
            let name = between(chunk, "<name>", "</name>").map(|n| n.trim_start_matches("<![CDATA[").trim_end_matches("]]>").to_string()).unwrap_or_default();
            let at = between(chunk, "<unlockTimestamp>", "</unlockTimestamp>").and_then(|t| t.trim().parse().ok()).unwrap_or(0);
            won.push((name, at));
        }
    }
    if out.total == 0 { return None; }
    won.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
    out.recent = won.into_iter().take(3).collect();
    Some(out)
}

fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let start = s.find(a)? + a.len();
    let end = s[start..].find(b)? + start;
    Some(&s[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_bits_against_the_schema() {
        let schema = Kv::Obj(vec![("10".into(), Kv::Obj(vec![("stats".into(), Kv::Obj(vec![
            ("1".into(), Kv::Obj(vec![("bits".into(), Kv::Obj(vec![
                ("0".into(), Kv::Obj(vec![("display".into(), Kv::Obj(vec![("name".into(), Kv::Obj(vec![("english".into(), Kv::Str("First".into())), ("french".into(), Kv::Str("Premier".into()))]))]))])),
                ("1".into(), Kv::Obj(vec![])),
                ("2".into(), Kv::Obj(vec![])),
            ]))])),
        ]))]))]);
        let user = Kv::Obj(vec![("cache".into(), Kv::Obj(vec![
            ("1".into(), Kv::Obj(vec![("data".into(), Kv::Int(0b101)), ("AchievementTimes".into(), Kv::Obj(vec![("0".into(), Kv::Int(100)), ("2".into(), Kv::Int(200))]))])),
        ]))]);
        let a = from_files(&schema, Some(&user), 10, true).unwrap();
        assert_eq!((a.unlocked, a.total), (2, 3));
        assert_eq!(a.recent[1], ("Premier".to_string(), 100));
    }

    #[test]
    fn reads_a_public_profile() {
        let xml = r#"<achievements><achievement closed="1"><name><![CDATA[Won]]></name><unlockTimestamp>5</unlockTimestamp></achievement><achievement closed="0"><name>No</name></achievement></achievements>"#;
        let a = parse_public(xml).unwrap();
        assert_eq!((a.unlocked, a.total, a.recent[0].0.as_str()), (1, 2, "Won"));
    }

    #[test]
    #[ignore]
    fn achievements_on_this_pc() {
        let steam = crate::extras::steam_root().expect("Steam");
        for e in std::fs::read_dir(steam.join("appcache/stats")).unwrap().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let Some(id) = n.strip_prefix("UserGameStatsSchema_").and_then(|s| s.strip_suffix(".bin")) else { continue };
            println!("{id}: {:?}", local(&steam, id.parse().unwrap(), true));
        }
    }
}
