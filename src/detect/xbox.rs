//! Microsoft Store and PC Game Pass games: the installed app packages, asked of PowerShell
//! once (`Get-AppxPackage`, with `Get-StartApps` for the names the Start menu shows). A package
//! is a game when it ships a `MicrosoftGame.config` (every Game Pass PC game) or its manifest
//! registers with the Windows game explorer or Xbox Live.

use std::path::{Path, PathBuf};

use crate::game::{Game, Launch, Source, created};

const SCRIPT: &str = r#"
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$ErrorActionPreference = 'SilentlyContinue'
$names = @{}
Get-StartApps | ForEach-Object { $names[$_.AppID] = $_.Name }
$r = foreach ($p in Get-AppxPackage -PackageTypeFilter Main) {
  if ($p.IsFramework -or $p.SignatureKind -ne 'Store' -or -not $p.InstallLocation) { continue }
  $loc = $p.InstallLocation
  $text = [IO.File]::ReadAllText((Join-Path $loc 'AppxManifest.xml'))
  $config = Test-Path -LiteralPath (Join-Path $loc 'MicrosoftGame.config')
  if (-not $config -and $text -notmatch 'gameExplorer|xboxlive') { continue }
  $x = [xml]$text
  $apps = @($x.Package.Applications.Application | ForEach-Object {
    $id = "$($p.PackageFamilyName)!$($_.Id)"
    [pscustomobject]@{ id = $id; name = $names[$id] }
  })
  [pscustomobject]@{
    pfn = $p.PackageFamilyName; name = $p.Name; dir = $loc; config = $config
    display = [string]$x.Package.Properties.DisplayName; logo = [string]$x.Package.Properties.Logo; apps = $apps
  }
}
ConvertTo-Json -InputObject @($r) -Depth 4 -Compress
"#;

/// Xbox's own apps and services, which look like games to the test above.
const NOT_GAMES: &[&str] = &["Microsoft.Xbox", "Microsoft.GamingApp", "Microsoft.GamingServices", "Microsoft.Edge.GameAssist", "Microsoft.Windows"];

pub fn detect() -> Vec<Game> {
    let Some(json) = run() else { return Vec::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json.trim_start_matches('\u{feff}')) else { return Vec::new() };
    let items = match &v {
        serde_json::Value::Array(a) => a.clone(),
        serde_json::Value::Object(_) => vec![v.clone()],
        _ => Vec::new(),
    };
    items.iter().filter_map(game).collect()
}

fn game(p: &serde_json::Value) -> Option<Game> {
    let s = |k: &str| p.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let (pfn, package) = (s("pfn"), s("name"));
    if pfn.is_empty() || NOT_GAMES.iter().any(|n| package.starts_with(n)) { return None; }
    let apps: Vec<&serde_json::Value> = match p.get("apps") {
        Some(serde_json::Value::Array(a)) => a.iter().collect(),
        Some(o @ serde_json::Value::Object(_)) => vec![o],
        _ => Vec::new(),
    };
    let app = apps.first()?;
    let app_id = app.get("id").and_then(|x| x.as_str())?.to_string();
    let start_name = apps.iter().find_map(|a| a.get("name").and_then(|x| x.as_str())).unwrap_or("");
    let display = s("display");
    let name = [start_name, display.as_str()]
        .into_iter()
        .find(|n| !n.is_empty() && !n.starts_with("ms-resource:"))
        .map(String::from)
        .unwrap_or_else(|| package.rsplit('.').next().unwrap_or(&package).to_string());
    let loc = PathBuf::from(s("dir"));
    // Game Pass installs into `XboxGames\<Game>\Content`: the game's folder is the one above.
    let dir = if loc.file_name().is_some_and(|n| n.eq_ignore_ascii_case("Content")) {
        loc.parent().map(Path::to_path_buf).unwrap_or_else(|| loc.clone())
    } else {
        loc.clone()
    };
    Some(Game {
        id: format!("xbox:{pfn}"),
        name,
        source: Source::Xbox,
        launch: Launch::Uri(format!("shell:AppsFolder\\{app_id}")),
        icon: logo(&loc, &s("logo")),
        installed: created(&dir),
        dir: Some(dir),
        ..Default::default()
    })
}

/// The manifest names `Assets\StoreLogo.png`; on disk it's `StoreLogo.scale-200.png` and the like.
fn logo(dir: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() { return None; }
    let path = dir.join(rel);
    if path.is_file() { return Some(path); }
    let (folder, stem) = (path.parent()?, path.file_stem()?.to_string_lossy().to_lowercase());
    std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            n.starts_with(&format!("{stem}.")) && n.ends_with(".png") && !n.contains("contrast")
        })
        .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

#[cfg(windows)]
fn run() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(not(windows))]
fn run() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_packages() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"pfn":"Microsoft.Halo_8wekyb3d8bbwe","name":"Microsoft.Halo","dir":"D:\\XboxGames\\Halo\\Content","config":true,
                "display":"ms-resource:Title","logo":"","apps":[{"id":"Microsoft.Halo_8wekyb3d8bbwe!Game","name":"Halo Infinite"}]}"#,
        )
        .unwrap();
        let g = game(&v).unwrap();
        assert_eq!(g.name, "Halo Infinite");
        assert_eq!(g.dir, Some(PathBuf::from(r"D:\XboxGames\Halo")));
        assert_eq!(g.launch, Launch::Uri(r"shell:AppsFolder\Microsoft.Halo_8wekyb3d8bbwe!Game".into()));
        let app: serde_json::Value = serde_json::from_str(r#"{"pfn":"x","name":"Microsoft.XboxApp","apps":[{"id":"x!A"}]}"#).unwrap();
        assert!(game(&app).is_none());
    }
}
