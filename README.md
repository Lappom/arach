<img src="assets/icon.svg" width="64" height="64" alt="">

# Arach

One launcher for every game on your PC: Steam, Epic Games, GOG, EA, Ubisoft Connect,
Riot Games, Battle.net, Xbox / Microsoft Store (Game Pass), Amazon Games, Rockstar, itch.io,
Heroic and Legendary, across every drive, plus console games launched with their emulator:
DS, 3DS, Game Boy Advance, GameCube, Wii, PlayStation 1, 2 and 3, Switch, and anything
RetroArch knows about. Written in Rust with [Rune](https://github.com/anomalyco/rune)
(native GPU rendering, no webview).

> Windows only. The interface is currently in French.

## Download

Grab `arach.exe` from the [latest release](https://github.com/Lappom/arach/releases/latest) and run it.
No installer; Arach updates itself (see [Updates](#updates)).

## Pages

- **Home**: the last game you launched, front and center (artwork, logo, playtime, Play), then
  *Recently played*, *Favorites*, *Recently installed*, and **Storage**: one card per drive with
  its free space and each store's share.
- **Library**: covers or a detailed list (playtime, last session, size), by store, favorites,
  collection or drive, sorted by recency, name, size, playtime or rating.
  Combinable **filters**: status, genres, release year, collection, "never launched", "not
  launched in 6 months"; each active filter is a chip you remove with one click.
  **Multi-select** (*Select* button or `Ctrl+click`): favorite, collect, set status, hide or merge
  several games at once. Games you own but haven't installed (Steam, Epic, GOG) are listed too,
  greyed out, with an **Install** button that opens their store; the *To install* shelf groups them.
- **Collections**: your own lists ("To finish", "Co-op", "Retro"…) in the sidebar, alongside
  Favorites. Add a game by dragging its cover (or row) onto the collection's name, from the
  right-click menu, or from its page. Each collection is also a Library shelf, where it can be
  renamed or deleted (the games stay in the library).
- **Game page**: artwork, description, developer, release date, genres, Metacritic score,
  playtime, size, folder, launch options, and for Steam: verify files, store page, *Update
  pending* badge. **Saves**: where the game keeps them, a dated backup in one click, and every
  backup restorable. **Screenshots**: Steam, Xbox Game Bar and NVIDIA. **Steam achievements**,
  **How long to beat** (HowLongToBeat, against the time already played), **Mods** (folder, Nexus
  Mods) and **Uninstall**, for every store. **My progress**: status (not started, playing,
  finished, 100 %, abandoned), 5-star rating, collections and free-form notes. **Tracked
  sessions**: recent sessions, the total and this week. **Versions**: the same game seen by
  several stores, each playable, and *Split* when it isn't actually the same game. **Edit**: name,
  description, developers, date and genres fixed by hand. **Images**: pick the cover, background
  or logo from Steam and SteamGridDB results, use a file, or drop an image on the page (portrait:
  cover, landscape: background). The page takes on the tint of the game's artwork.
- **Settings**: General (after launch: minimize / stay / quit, start page, shortcuts), Library
  (enabled stores, extra game folders, scan), Appearance (theme, Play button color, cover size,
  layout), Online (covers and descriptions, language, SteamGridDB key, cache), Hidden games, About.
  Library › Consoles and Emulators: each console and the emulator that runs it, detected
  automatically or picked by hand.

## Shortcuts

| Key | Action |
| --- | --- |
| *type anywhere* / `Ctrl+K` | Search (substring, initials `kcd`, letters in order) |
| `Enter` in search | Launch the first result |
| double-click a cover | Play |
| right-click a game | Play · Open page · Favorite · Folder · Collections · Edit · Cover · Merge · Hide |
| `Ctrl+click` a game | Select it (then `Ctrl+A`: all, `Esc`: cancel) |
| drag a cover onto the sidebar | Add it to Favorites or a collection |
| `Alt+←` / `Esc` | Previous page |
| `Ctrl+1` `Ctrl+2` `Ctrl+3` | Home, Library, Favorites |
| `Ctrl+,` | Settings |
| `F5` / `Ctrl+R` | Rescan |
| `F11` | Toggle fullscreen |

Dropping an `.exe`, a `.lnk` shortcut or a `.url` link on the window adds it to the library.

## Detection

Each store is read from its own manifests rather than by crawling drives, so games are found
wherever they're installed (C:, D:, F:…) in a few milliseconds:

| Store | Source | Launch |
| --- | --- | --- |
| Steam | `libraryfolders.vdf` → `appmanifest_*.acf`; playtime from `userdata\*\config\localconfig.vdf` | `steam://rungameid/<id>` |
| Epic Games | `ProgramData\Epic\…\Manifests\*.item` | `com.epicgames.launcher://apps/…?action=launch` |
| GOG | `HKLM\…\GOG.com\Games` (DLC ignored) | game executable |
| Ubisoft | `HKLM\…\Ubisoft\Launcher\Installs` | `uplay://launch/<id>/0` |
| Riot Games | `Riot Game <product>.<patchline>` entries + `RiotClientInstalls.json` | `RiotClientServices --launch-product=…` |
| EA app | EA uninstall entries with `__Installer\installerdata.xml` | game executable |
| Battle.net | Blizzard uninstall entries (`--uid=`) | `Battle.net.exe --exec="launch <code>"` |
| Xbox / Microsoft Store | `Get-AppxPackage` (Store packages with `MicrosoftGame.config` or declared as games) + Start menu names | `shell:AppsFolder\<app>` |
| Amazon Games | `Amazon Game Remover.exe -p <id>` uninstall entries | `amazon-games://play/<id>` |
| Rockstar | launcher uninstall entries (`-uninstall=<title>`) | `Launcher.exe -launchTitleInFolder` |
| itch.io | `.itch\receipt.json.gz` receipts in `%APPDATA%\itch\apps` and the locations in `butler.db`; cover from the receipt | game executable |
| Heroic | Legendary, GOG and Nile `installed.json` in `%APPDATA%\heroic` | `heroic://launch?appName=…&runner=…` |
| Legendary | `~\.config\legendary\installed.json` | `legendary launch <app>` (or the executable) |
| Folders | each subfolder of a folder picked in Settings | executable found in the folder |

**Consoles.** ROMs and disc images are looked for in the folders emulators already know about
(their game list and recent files) and in folders added in Settings. The title and game code are
read from the file when the format carries them:

| Console | Files | Read from the file | Emulator (otherwise a RetroArch core) |
| --- | --- | --- | --- |
| Nintendo DS | `.nds` | title, publisher, icon (banner) | DeSmuME |
| Nintendo 3DS | `.3ds` `.cci` `.cxi` `.cia` (decrypted) `.3dsx`; updates and DLC ignored | title, publisher, icon (SMDH) | Azahar |
| Game Boy Advance | `.gba` | code (`AXVE`) | mGBA |
| GameCube / Wii | `.iso` `.gcm` `.rvz` `.wia` `.wbfs` `.ciso` `.gcz` | ID (`GZLE01`), title | Dolphin (`-b -e`) |
| PlayStation | `.cue` `.bin` `.chd` `.pbp` `.m3u`; tracks and discs 2+ grouped | serial (`SCUS-94455`) | DuckStation |
| PlayStation 2 | `.iso` `.chd` `.cso` | serial (`SLUS-20062`) | PCSX2 |
| PlayStation 3 | game folders (`PS3_GAME\PARAM.SFO`, `dev_hdd0\game`) + RPCS3's `games.yml` | title, serial, icon, background | RPCS3 (`--no-gui`) |
| Switch | `.nsp` `.xci` `.nsz` `.xcz`; updates and DLC ignored | title ID (file name) | Ryujinx |
| RetroArch | `*.lpl` playlists | exact libretro name | the playlist's core |

An `.iso` image is recognized by its header (Nintendo, or the PlayStation `SYSTEM.CNF`); a
compressed image (`.chd`, `.gcz`) by the emulator that lists it or the folder it's in (`PS2`,
`PSX`…). Emulators are found in Program Files, `%APPDATA%`, the user's folders, next to the ROMs
and at drive roots, or picked in Settings › Library › Emulators.

**Duplicates.** A game seen by two stores in the same folder (an EA game bought on Steam) shows
up once, with its versions on its page; playtime adds up. If the automatic merge gets it wrong,
*Split* keeps the version separate; conversely, *Merge* (multi-select or right-click › Merge
with…) joins games Arach didn't match. Stores are scanned in parallel; the last scan is cached,
so the library shows up on the first frame and then refreshes in the background.

### Playtime

Only Steam tracks playtime; Arach measures it for every game. After a launch, it looks every 2 s
(for up to 5 min, while the launcher starts the game) for a process started from the game's
folder, or the emulator for a ROM. It then follows it every 5 s. The session ends after 15 s with
no process at all, which gives a launcher time to hand over to the game. Each session (start,
end) goes into `user.json`. Sessions under 10 s don't count. The displayed and sorted time is the
larger of the two: the store's or the measured one. Arach has to stay open: with "Quit Arach"
after launch, nothing is measured.

### Now playing, before and after

A running game appears in the sidebar (*Now playing*) with its session time and a **Stop**
button, which also replaces **Play** on its page: it ends every process in the game's folder (or
the emulator), then the session closes normally.

Settings › General › *Before and after each game* sets what Arach does around a launch, for all
games. Each game page can have its own settings ("Specific to this game"):

| Action | Before | After exit |
| --- | --- | --- |
| Script | `cmd /C <command>` with no window, waited on for up to 1 min | started without waiting |
| Close Discord | Discord, PTB or Canary terminated | restarted minimized (via `Update.exe`) |
| Power plan | `PowerSetActiveScheme` (plans read from the PC) | previous plan |
| Resolution | primary display, without writing to the registry | usual resolution |

Actions only apply to games whose exit Arach can see (not `.lnk` or `.url` shortcuts added by
hand). With after-game actions, "Quit Arach" becomes "Minimize" for that launch; if Arach is
closed during the game, it restores everything on its way out.

**Discord Rich Presence**: your Discord profile shows the game launched from Arach (name, store,
duration, Steam image when there is one), whatever its store, as long as the Discord desktop app
is open. Arach talks to Discord over its local pipe (`\\.\pipe\discord-ipc-N`) under its own
Discord application, with nothing to set up (Settings › General › Discord turns it off).

### Owned but not installed

| Store | Source | Install |
| --- | --- | --- |
| Steam | games in `appcache\librarycache` and those with playtime in `localconfig.vdf`, named and filtered (games only) by `appcache\appinfo.vdf` | `steam://install/<id>` |
| Epic Games | the launcher's `catcache.bin` (base64 catalog), `games` category | `com.epicgames.launcher://apps/…?action=install` |
| GOG | GOG Galaxy database (`galaxy-2.0.db`, `LibraryReleases` table), DLC and packs ignored | `goggalaxy://openGameView/<id>` |

These are the lists launchers keep cached: a game never shown in its launcher may be missing.
Can be turned off in Settings › Library.

## Artwork and metadata

Steam keeps its games' artwork locally (`appcache\librarycache`). For everything else, Arach looks
the game up by name on the public Steam Store API and downloads the cover, hero art and logo,
along with the description, developer, release date and genres. Games not on Steam (League of
Legends, Teamfight Tactics, Valorant…) are looked up the same way on the Epic Games Store (cover,
hero art, description, developer, date). Console games are looked up by the code read from the
file (`CPUF`, `GZLE01`, `SLUS-20062`…): cover from GameTDB (DS, 3DS, GameCube/Wii, PS3) or
libretro-thumbnails, fitted to 2:3 over a blurred background, and title screen (or snapshot) from
libretro-thumbnails, matched through libretro's No-Intro and Redump databases (cached 30 days) or
the exact file name. The cover itch.io provides is reused. With a SteamGridDB key, *Change images*
also offers its community's artwork. Only game names are sent; a name that doesn't match exactly
is skipped rather than showing the wrong cover. Can be turned off in Settings › Online.

| Where | What |
| --- | --- |
| `%APPDATA%\Arach` | `settings.json`, `user.json` (favorites, hidden games, launch options, added games, play sessions, collections, each game's status, rating and notes, edited metadata, chosen images, merges and splits), `covers\` (chosen images), `library.json` (last scan) |
| `%APPDATA%\Arach\saves` | one save backup per dated folder, with `arach-backup.json` (where each file came from) |
| `%LOCALAPPDATA%\Arach` | `store.json` (Steam Store info, measured sizes), `art\` (artwork), `ludusavi-manifest.yaml`, `hltb.json` |

### Game page: the rest

| What | Source |
| --- | --- |
| Saves | The [Ludusavi](https://github.com/mtkennerly/ludusavi-manifest) manifest (from PCGamingWiki): Ludusavi's own copy if it's installed, otherwise downloaded and kept 7 days; the game is matched by Steam ID, then by name. Failing that, the `Game data/saves` templates on the game's PCGamingWiki page. Only Windows paths that exist are kept; the whole game folder never counts as a save. |
| Back up, restore | Copied to `%APPDATA%\Arach\saves\<game>\<date>`. Restore replaces the current saves after backing them up in turn (marked *made before a restore*); refused while the game is running. Restore and delete need a second press. |
| Screenshots | Steam: `userdata\<account>\760\remote\<appid>\screenshots`. Game Bar: `Videos\Captures`, files named after the game's window followed by the date. NVIDIA: `Videos\<Game>` and `Videos\NVIDIA\<Game>`. |
| Mods | First folder found among `Mods`, `BepInEx\plugins`, `Content\Paks\~mods`…; otherwise the game folder. Nexus Mods: search by name. |
| Steam achievements | `appcache\stats\UserGameStatsSchema_<appid>.bin` (the list) and `UserGameStats_<account>_<appid>.bin` (unlocked bits and their date), in binary KeyValues; otherwise the public profile's XML page. |
| Update pending | The `appmanifest`'s `StateFlags` (bits 2, 256, 512, 1024) or a `TargetBuildID` different from `buildid`. |
| HowLongToBeat | The site's own search (`/api/search/site`, with the token from `/api/search/site/init`); only an identical name (or alias) counts. Kept 30 days. |
| Uninstall | Steam: `steam://uninstall/<id>`. Others: the `UninstallString` of the registry's uninstall entries, matched by install folder, then by name; Xbox: Installed apps. A second press confirms. |

## Updates

Arach updates itself from [GitHub Releases](https://github.com/Lappom/arach/releases). On
startup, it asks GitHub for the latest published version; if it's newer, its `arach.exe` downloads
in the background to `%LOCALAPPDATA%\Arach\update\`, and its size and hash (`arach.exe.sha256`)
are checked. *Arach X is ready* then appears in the sidebar; **Restart and install** (Settings ›
About) swaps the new exe in for the old one and restarts Arach. Nothing installs without that
click. The startup check can be turned off in About, and it never runs in a debug build.

If Arach lives in a protected folder (`Program Files`), it can't replace itself: it says so and
points to the release page.

Publishing a version: set `version` to `X.Y.Z` in `Cargo.toml`, then push the tag `vX.Y.Z`.
The `.github/workflows/release.yml` workflow builds, tests and publishes `arach.exe` and
`arach.exe.sha256`.

## Building

```sh
cargo build --release      # target/release/arach.exe
cargo test                 # parsers, search, executable picking, dates
cargo test scan_this_pc -- --ignored --nocapture     # real detection on this PC
cargo test owned_on_this_pc -- --ignored --nocapture # owned but not installed games (Steam, Epic, GOG)
cargo test lookup_cyberpunk -- --ignored --nocapture # Steam Store request
cargo test lookup_league -- --ignored --nocapture    # Epic Games Store request
cargo test scan_roms -- --ignored --nocapture         # emulators and ROMs on this PC
cargo test lookup_roms -- --ignored --nocapture        # GameTDB covers and libretro images
cargo test latest_release -- --ignored --nocapture     # latest release on GitHub
cargo test saves_on_this_pc -- --ignored --nocapture   # saves of this PC's games (Ludusavi, PCGamingWiki)
cargo test achievements_on_this_pc -- --ignored --nocapture # Steam achievements read locally
cargo test lookup_celeste -- --ignored --nocapture     # HowLongToBeat request
```

The toolchain (`rust-toolchain.toml`) is installed by rustup on the first build.

Headless screenshots, with a separate data folder so the real library isn't touched:

```sh
ARACH_DATA_DIR=/tmp/arach target/debug/arach --snap out.png --scene game-steam --frames 150
```

Scenes: `library`, `list`, `search`, `game`, `game-steam`, `playing`, `game-3ds`, `library-ds`, `settings`, `settings-library`,
`settings-look`, `settings-about`, `update`, `light`, `toast`.

## License

[MIT](LICENSE). Display font: Big Shoulders Display (SIL OFL 1.1, `assets/fonts/`).
