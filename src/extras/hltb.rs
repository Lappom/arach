//! HowLongToBeat: how long the story, the story plus extras and 100 % take, as its players
//! report. The site has no public API; its own search asks `/api/search/site/init` for a token
//! (bound to the user agent) and posts the query with it. Answers are kept 30 days.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::screenshots::squash;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const SITE: &str = "https://howlongtobeat.com";
const KEEP_SECS: i64 = 30 * 86_400;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hltb {
    pub id: u64,
    pub name: String,
    /// Minutes; 0 when nobody reported it.
    pub main: u64,
    pub plus: u64,
    pub full: u64,
}

impl Hltb {
    pub fn url(&self) -> String {
        format!("{SITE}/game/{}", self.id)
    }
}

/// Game id → (when asked, the answer: `None` when the site doesn't know the game).
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Cache(pub HashMap<String, (i64, Option<Hltb>)>);

fn cache_path() -> std::path::PathBuf {
    crate::store::cache_dir().join("hltb.json")
}

pub fn load_cache() -> Cache {
    std::fs::read(cache_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_cache(c: &Cache) {
    let path = cache_path();
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    if let Ok(b) = serde_json::to_vec(c) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, b).is_ok() { let _ = std::fs::rename(tmp, path); }
    }
}

impl Cache {
    /// The cached answer if still fresh: `Some(None)` means "asked, not found".
    pub fn fresh(&self, id: &str, now: i64) -> Option<Option<Hltb>> {
        self.0.get(id).filter(|(at, _)| now - at < KEEP_SECS).map(|(_, h)| h.clone())
    }
}

/// Looks `name` up. `Err` when the site couldn't be asked (network, changed API), `Ok(None)`
/// when it answered without that game.
pub fn lookup(name: &str) -> Result<Option<Hltb>, String> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(12)).user_agent(UA).build();
    let init: serde_json::Value = agent.get(&format!("{SITE}/api/search/site/init?t={}", crate::store::now() * 1000))
        .set("Referer", &format!("{SITE}/"))
        .call().map_err(|e| e.to_string())?
        .into_json().map_err(|e| e.to_string())?;
    let token = init.get("token").and_then(|t| t.as_str()).ok_or("HowLongToBeat n'a pas donné de jeton")?;
    let clean = name.replace(['™', '®', '©'], "");
    let terms: Vec<&str> = clean.split_whitespace().collect();
    let body = serde_json::json!({
        "searchType": "games",
        "searchTerms": terms,
        "searchPage": 1,
        "size": 20,
        "searchOptions": {
            "games": {
                "userId": 0, "platform": "", "sortCategory": "popular", "rangeCategory": "main",
                "rangeTime": { "min": null, "max": null },
                "gameplay": { "perspective": "", "flow": "", "genre": "" },
                "year": "", "modifier": ""
            },
            "users": { "sortCategory": "postcount" },
            "lists": { "sortCategory": "follows" },
            "filter": "", "sort": 0, "randomizer": 0
        },
        "useCache": true
    });
    let resp: serde_json::Value = agent.post(&format!("{SITE}/api/search/site"))
        .set("Referer", &format!("{SITE}/"))
        .set("Origin", SITE)
        .set("x-auth-token", token)
        .send_json(body).map_err(|e| e.to_string())?
        .into_json().map_err(|e| e.to_string())?;
    let data = resp.get("data").and_then(|d| d.as_array()).ok_or("Réponse inattendue de HowLongToBeat")?;
    Ok(pick(name, data))
}

/// The result whose name (or alias) is the game's, symbols aside; nothing rather than a guess.
fn pick(name: &str, data: &[serde_json::Value]) -> Option<Hltb> {
    let want = squash(name);
    let found = data.iter().find(|d| {
        let n = d.get("game_name").and_then(|v| v.as_str()).unwrap_or("");
        let alias = d.get("game_alias").and_then(|v| v.as_str()).unwrap_or("");
        squash(n) == want || alias.split(',').any(|a| !a.trim().is_empty() && squash(a) == want)
    })?;
    let mins = |k: &str| found.get(k).and_then(|v| v.as_u64()).unwrap_or(0) / 60;
    Some(Hltb {
        id: found.get("game_id")?.as_u64()?,
        name: found.get("game_name")?.as_str()?.to_string(),
        main: mins("comp_main"),
        plus: mins("comp_plus"),
        full: mins("comp_100"),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_only_the_same_name() {
        let data: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"game_id":2,"game_name":"Celeste Classic","game_alias":"","comp_main":600},
                {"game_id":1,"game_name":"Celeste","game_alias":"","comp_main":29951,"comp_plus":52996,"comp_100":141105}]"#,
        ).unwrap();
        let h = super::pick("Celeste™", &data).unwrap();
        assert_eq!((h.id, h.main, h.full), (1, 499, 2351));
        assert!(super::pick("Celeste 2", &data).is_none());
    }

    #[test]
    #[ignore]
    fn lookup_celeste() {
        let h = super::lookup("Celeste").unwrap().unwrap();
        println!("{h:?}");
        assert!(h.main > 60);
    }
}
