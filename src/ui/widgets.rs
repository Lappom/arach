//! The pieces every page shares: buttons, switches, segmented choices, covers, labels and the
//! wording of sizes and dates.

use rune::{Color, El, SurfaceFit, Tx, col, el, row, text};

use super::icons::{Icon, icon};
use super::theme::{Palette, display, num, store_color, ui};
use crate::app::{Overlay, Page, State, fnv};
use crate::game::{Game, Source};
use crate::store::Status;

// ---------------------------------------------------------------- words

pub fn size(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if bytes == 0 {
        "—".into()
    } else if b >= 1024.0 * GB {
        format!("{:.2} To", b / 1024.0 / GB).replace('.', ",")
    } else if b >= GB {
        format!("{:.1} Go", b / GB).replace('.', ",")
    } else {
        format!("{:.0} Mo", (b / 1024.0 / 1024.0).max(1.0))
    }
}

pub fn playtime(min: u64) -> String {
    match min {
        0 => "—".into(),
        m if m < 60 => format!("{m} min"),
        m if m < 600 => format!("{} h {:02}", m / 60, m % 60),
        m => format!("{} h", m / 60),
    }
}

pub fn ago(ts: i64) -> String {
    if ts <= 0 { return "Jamais".into(); }
    let d = (crate::store::now() - ts).max(0);
    match d {
        d if d < 120 => "À l'instant".into(),
        d if d < 3600 => format!("Il y a {} min", d / 60),
        d if d < 86_400 => format!("Il y a {} h", d / 3600),
        d if d < 2 * 86_400 => "Hier".into(),
        d if d < 30 * 86_400 => format!("Il y a {} jours", d / 86_400),
        d if d < 365 * 86_400 => format!("Il y a {} mois", d / (30 * 86_400)),
        d => {
            let y = d / (365 * 86_400);
            format!("Il y a {y} an{}", if y > 1 { "s" } else { "" })
        }
    }
}

pub fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

// ---------------------------------------------------------------- controls

/// The play button: the accent, filled, with a soft light from above and a glow under it.
pub fn play_button(p: &Palette, key: impl Into<String>, label: &str, big: bool) -> El {
    let (h, px, size) = if big { (46.0, 24.0, 15.0) } else { (34.0, 16.0, 13.5) };
    row()
        .key(key.into())
        .role("button")
        .label(label.to_string())
        .h(h)
        .px(px)
        .gap(9.0)
        .radius(h / 2.0)
        .center()
        .fill(p.accent)
        .linear_gradient(std::f32::consts::PI, [(0.0, Color::WHITE.alpha(0.14)), (0.55, Color::WHITE.alpha(0.0))])
        .border(1.0, Color::WHITE.alpha(0.12))
        .shadow(if big { 8.0 } else { 4.0 }, if big { 24.0 } else { 14.0 }, p.accent.alpha(if p.dark { 0.32 } else { 0.28 }))
        .hover(Color::WHITE.alpha(0.1))
        .press(Color::BLACK.alpha(0.08))
        .press_scale(0.97)
        .child(icon(Icon::Play, if big { 15.0 } else { 12.0 }, p.on_accent))
        .child(text(label.to_string()).font(ui(size, 650.0)).color(p.on_accent))
}

/// Where the play button was while the game runs: the same shape, in the danger colour, to end it.
pub fn stop_button(p: &Palette, key: impl Into<String>, label: &str, big: bool) -> El {
    let (h, px, size) = if big { (46.0, 24.0, 15.0) } else { (34.0, 16.0, 13.5) };
    row()
        .key(key.into())
        .role("button")
        .label(label.to_string())
        .h(h)
        .px(px)
        .gap(9.0)
        .radius(h / 2.0)
        .center()
        .fill(p.danger)
        .linear_gradient(std::f32::consts::PI, [(0.0, Color::WHITE.alpha(0.12)), (0.55, Color::WHITE.alpha(0.0))])
        .border(1.0, Color::WHITE.alpha(0.12))
        .hover(Color::WHITE.alpha(0.1))
        .press(Color::BLACK.alpha(0.08))
        .press_scale(0.97)
        .child(icon(Icon::Stop, if big { 14.0 } else { 11.0 }, Color::WHITE))
        .child(text(label.to_string()).font(ui(size, 650.0)).color(Color::WHITE))
}

/// A secondary action over artwork: translucent, the same height as the play button beside it.
pub fn hero_button(p: &Palette, key: impl Into<String>, label: &str, glyph: Option<Icon>) -> El {
    let mut b = row()
        .key(key.into())
        .role("button")
        .label(label.to_string())
        .h(46.0)
        .px(20.0)
        .gap(8.0)
        .radius(23.0)
        .center()
        .fill(p.text.alpha(0.1))
        .border(1.0, p.text.alpha(0.08))
        .hover(p.text.alpha(0.07))
        .press_scale(0.97);
    if let Some(g) = glyph {
        b = b.child(icon(g, 16.0, p.text));
    }
    b.child(text(label.to_string()).font(ui(14.0, 600.0)).color(p.text))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Secondary,
    /// The one action a row exists for, in the accent colour.
    Primary,
    Danger,
}

/// A rectangular button for forms, settings and dialogs.
pub fn button(p: &Palette, key: impl Into<String>, label: &str, glyph: Option<Icon>, tone: Tone) -> El {
    let (fill, ink, border) = match tone {
        Tone::Secondary => (p.field, p.text, p.line),
        Tone::Primary => (p.accent, p.on_accent, Color::WHITE.alpha(0.12)),
        Tone::Danger => (p.danger, Color::WHITE, Color::WHITE.alpha(0.1)),
    };
    let mut b = row()
        .key(key.into())
        .role("button")
        .label(label.to_string())
        .h(34.0)
        .px(14.0)
        .gap(8.0)
        .radius(5.0)
        .center()
        .fill(fill)
        .border(1.0, border)
        .hover(if tone == Tone::Secondary { p.hover } else { Color::WHITE.alpha(0.1) })
        .press_scale(0.97);
    if tone == Tone::Secondary && !p.dark {
        b = b.shadow(1.0, 2.0, p.shadow.alpha(0.06));
    }
    if let Some(g) = glyph {
        b = b.child(icon(g, 15.0, if tone == Tone::Secondary { p.muted } else { ink }));
    }
    b.child(text(label.to_string()).font(ui(13.0, 600.0)).color(ink))
}

/// The usual quiet button.
pub fn ghost_button(p: &Palette, key: impl Into<String>, label: &str, glyph: Option<Icon>) -> El {
    button(p, key, label, glyph, Tone::Secondary)
}

/// A round button with only an icon; `help` names it for the tooltip and screen readers.
pub fn icon_button(p: &Palette, key: impl Into<String>, glyph: Icon, help: &str, size: f32) -> El {
    el().key(key.into())
        .role("button")
        .label(help.to_string())
        .help(help.to_string())
        .size(size, size)
        .radius(size / 2.0)
        .center()
        .fill(p.text.alpha(0.1))
        .border(1.0, p.text.alpha(0.07))
        .hover(p.text.alpha(0.07))
        .press_scale(0.94)
        .child(icon(glyph, (size * 0.4).max(14.0), p.text))
}

/// A square, borderless icon button for toolbars and rows.
pub fn tool_button(p: &Palette, key: impl Into<String>, glyph: Icon, help: &str, size: f32) -> El {
    el().key(key.into())
        .role("button")
        .label(help.to_string())
        .help(help.to_string())
        .size(size, size)
        .radius(5.0)
        .center()
        .hover(p.hover)
        .press_scale(0.94)
        .child(icon(glyph, (size * 0.5).min(17.0), p.muted))
}

pub fn switch(p: &Palette, key: impl Into<String>, label: &str, on: bool) -> El {
    let knob = el().key("knob").size(18.0, 18.0).radius(9.0).fill(Color::WHITE)
        .shadow(1.0, 3.0, Color::BLACK.alpha(0.25))
        .absolute(if on { 19.0 } else { 3.0 }, 3.0);
    el().key(key.into())
        .role("switch")
        .label(label.to_string())
        .checked(on)
        .focusable(true)
        .size(40.0, 24.0)
        .radius(12.0)
        .fill(if on { p.accent } else { p.text.alpha(if p.dark { 0.16 } else { 0.14 }) })
        .press_scale(0.96)
        .child(knob)
}

/// One choice among two or three, side by side. `pick(state, index, tx)` runs on a press.
pub fn segmented(
    p: &Palette,
    key: &str,
    labels: &[&str],
    selected: usize,
    pick: impl Fn(&mut State, usize, &mut Tx) + Clone + 'static,
) -> El {
    let mut bar = row().key(key.to_string()).role("radiogroup").focus_group().h(34.0).padding(3.0).gap(2.0).radius(6.0)
        .fill(p.text.alpha(if p.dark { 0.06 } else { 0.05 }))
        .border(1.0, p.line);
    for (i, label) in labels.iter().enumerate() {
        let on = i == selected;
        let pick = pick.clone();
        let mut seg = row()
            .key(format!("{key}-{i}"))
            .role("radio")
            .label(label.to_string())
            .checked(on)
            .focus_selected(on)
            .h(26.0)
            .px(12.0)
            .radius(4.0)
            .center()
            .fill(if on { if p.dark { p.raised } else { p.surface } } else { Color::TRANSPARENT })
            .hover(if on { Color::TRANSPARENT } else { p.hover })
            .child(text(label.to_string()).font(ui(12.5, if on { 600.0 } else { 500.0 })).color(if on { p.text } else { p.muted }))
            .on_press_tx(move |s: &mut State, tx: &mut Tx| {
                tx.instant();
                pick(s, i, tx)
            });
        if on {
            seg = seg.shadow(1.0, 3.0, p.shadow.alpha(if p.dark { 0.35 } else { 0.12 }));
        }
        bar = bar.child(seg);
    }
    bar
}

/// A shortcut, one key cap per key: `"Ctrl K"` draws Ctrl and K.
pub fn keycaps(p: &Palette, keys: &str) -> El {
    let mut r = row().gap(4.0).center_y();
    for k in keys.split(' ') {
        if k == "/" || k == "+" {
            r = r.child(text(k.to_string()).font(ui(11.5, 500.0)).color(p.faint));
            continue;
        }
        r = r.child(
            row().h(22.0).min_w(22.0).px(6.0).radius(6.0).center()
                .fill(p.text.alpha(if p.dark { 0.07 } else { 0.04 }))
                .border(1.0, p.line)
                .shadow(1.0, 0.0, p.text.alpha(if p.dark { 0.06 } else { 0.08 }))
                .child(text(k.to_string()).font(ui(11.5, 600.0)).color(p.muted)),
        );
    }
    r
}

pub fn store_dot(s: Source, size: f32) -> El {
    el().size(size, size).radius(size / 2.0).fill(store_color(s))
}

/// Each status's colour: grey not started, blue playing, green finished, gold for 100 %,
/// a muted red abandoned.
pub fn status_color(p: &Palette, st: Status) -> Color {
    let l = if p.dark { 0.74 } else { 0.6 };
    match st {
        Status::NotStarted => p.faint,
        Status::Playing => rune::oklch(l, 0.13, 250.0),
        Status::Finished => rune::oklch(l, 0.14, 150.0),
        Status::Completed => rune::oklch(if p.dark { 0.84 } else { 0.7 }, 0.15, 85.0),
        Status::Abandoned => rune::oklch(l, 0.1, 25.0),
    }
}

pub fn status_dot(p: &Palette, st: Status, size: f32) -> El {
    el().size(size, size).radius(size / 2.0).fill(status_color(p, st))
}

/// A section title with an optional action on the right.
pub fn section_head(p: &Palette, title: &str, more: Option<El>) -> El {
    row().w_full().h(28.0).center_y().gap(12.0)
        .child(text(title.to_string()).font(ui(17.0, 650.0)).color(p.text))
        .child(el().grow(1.0))
        .children(more)
}

/// "Tout voir ›" at the end of a section head.
pub fn link(p: &Palette, key: impl Into<String>, label: &str) -> El {
    row().key(key.into()).role("link").label(label.to_string()).h(28.0).pl(10.0).pr(6.0).gap(2.0).radius(5.0).center_y()
        .hover(p.hover)
        .child(text(label.to_string()).font(ui(12.5, 550.0)).color(p.muted))
        .child(icon(Icon::ChevronRight, 15.0, p.faint))
}

// ---------------------------------------------------------------- covers

/// The game's box art, or one drawn from its name and icon when there is none.
pub fn cover_art(s: &State, g: &Game, p: &Palette, w: f32, h: f32) -> El {
    let mut frame = el().w(w).h(h).radius(6.0).clip_rounded(6.0).child(generated_cover(s, g, p, w, h));
    let px = (w * 2.0).round() as u32;
    if let Some(path) = &g.art.cover
        && let Some(img) = s.images.borrow_mut().get(path, px)
    {
        frame = frame.child(el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).pixels_fit(img.pixels(), SurfaceFit::Fill));
    }
    frame
}

fn generated_cover(s: &State, g: &Game, p: &Palette, w: f32, h: f32) -> El {
    let hue = (fnv(&g.name) % 360) as f32;
    let top = rune::oklch(if p.dark { 0.34 } else { 0.78 }, 0.07, hue);
    let bottom = rune::oklch(if p.dark { 0.2 } else { 0.66 }, 0.05, hue + 35.0);
    let ink = if p.dark { Color::WHITE } else { rune::oklch(0.2, 0.02, hue) };
    let badge = w * 0.34;
    let mark = match s.icons.get(&g.id) {
        Some(img) if img.is_ready() => el().size(badge, badge).radius(badge * 0.22).clip_rounded(badge * 0.22).pixels(img.pixels()),
        _ => el().size(badge, badge).radius(badge * 0.22).fill(ink.alpha(0.12)).center().child(
            text(g.name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_default())
                .font(display(badge * 0.62))
                .color(ink.alpha(0.9)),
        ),
    };
    col()
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .gradient(top, bottom)
        .padding(w * 0.09)
        .gap(w * 0.06)
        .child(el().grow(1.0))
        .child(mark)
        .child(text(g.name.to_uppercase()).font(display((w * 0.13).clamp(14.0, 26.0))).color(ink).wrap().max_lines(3))
        .child(el().h(h * 0.04))
}

/// A press on a game: opens its page, or picks it while selecting. Ctrl+click starts picking.
pub fn open_or_select(s: &mut State, id: &str, tx: &mut Tx) {
    if s.selecting || tx.mods().cmd {
        s.toggle_selected(id, tx);
    } else {
        s.go(Page::Game(id.to_string()), tx);
    }
}

/// A round check for picked games.
pub fn check_box(p: &Palette, on: bool) -> El {
    el().size(22.0, 22.0).radius(11.0).center()
        .fill(if on { p.accent } else { Color::BLACK.alpha(0.35) })
        .border(1.5, if on { p.accent } else { Color::WHITE.alpha(0.75) })
        .children(on.then(|| icon(Icon::Check, 13.0, p.on_accent)))
}

/// A cover with its name under it: opens the game's page, plays on double-click.
pub fn cover_card(s: &State, g: &Game, p: &Palette, w: f32) -> El {
    let h = (w * 1.5).round();
    let (open_id, play_id, drag_id) = (g.id.clone(), g.id.clone(), g.id.clone());
    let owned = g.not_installed;
    // Hover: a hairline ring and a lift of light from the top, not a thick outline.
    let ring = el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(6.0)
        .border(1.0, Color::WHITE.alpha(if p.dark { 0.28 } else { 0.5 }))
        .linear_gradient(std::f32::consts::PI, [(0.0, Color::WHITE.alpha(0.08)), (0.4, Color::WHITE.alpha(0.0))])
        .group_opacity(0.0, 1.0);
    let edge = el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(6.0).border(1.0, p.text.alpha(if p.dark { 0.08 } else { 0.1 }));
    let badge = if owned {
        // Not installed: say so on the cover, always, and the press installs.
        row()
            .absolute(10.0, h - 42.0)
            .h(30.0)
            .px(12.0)
            .gap(6.0)
            .radius(15.0)
            .fill(Color::BLACK.alpha(0.62))
            .border(1.0, Color::WHITE.alpha(0.16))
            .center()
            .key("install")
            .role("button")
            .label(format!("Installer avec {}", g.source.label()))
            .hover(Color::WHITE.alpha(0.12))
            .press_scale(0.96)
            .child(text("Installer").font(ui(12.0, 650.0)).color(Color::WHITE))
            .on_press_tx({
                let id = g.id.clone();
                move |s: &mut State, tx: &mut Tx| s.launch(&id, tx)
            })
    } else {
        row()
            .absolute(w - 48.0, h - 48.0)
            .size(38.0, 38.0)
            .radius(19.0)
            .fill(p.accent)
            .border(1.0, Color::WHITE.alpha(0.14))
            .center()
            .shadow(6.0, 16.0, Color::BLACK.alpha(0.4))
            .group_opacity(0.0, 1.0)
            .child(icon(Icon::Play, 14.0, p.on_accent))
    };
    let entry = s.user.entry(&g.id);
    let status = (entry.status != Status::NotStarted).then(|| {
        row().absolute(8.0, if s.is_favorite(&g.id) { 40.0 } else { 8.0 }).h(22.0).px(8.0).gap(5.0).radius(11.0).center_y()
            .fill(Color::BLACK.alpha(0.55)).border(1.0, Color::WHITE.alpha(0.12))
            .child(status_dot(p, entry.status, 6.0))
            .child(text(entry.status.label()).font(ui(11.0, 600.0)).color(Color::WHITE))
    });
    let fav = s.is_favorite(&g.id).then(|| {
        el().absolute(8.0, 8.0).size(26.0, 26.0).radius(13.0).fill(Color::BLACK.alpha(0.5)).border(1.0, Color::WHITE.alpha(0.12)).center()
            .child(icon(Icon::StarFilled, 13.0, rune::oklch(0.86, 0.15, 85.0)))
    });
    let art = el()
        .key("art")
        .w(w)
        .h(h)
        .radius(6.0)
        .shadow(8.0, 22.0, Color::BLACK.alpha(if p.dark { 0.4 } else { 0.14 }))
        .child(cover_art(s, g, p, w, h).opacity(if owned { 0.42 } else { 1.0 }))
        .child(edge)
        .child(ring)
        .children(fav)
        .children(status)
        .children((g.update_pending && !owned).then(|| super::extras::update_badge(p, true).absolute(8.0, h - 28.0)))
        .child(badge)
        .children(s.selecting.then(|| {
            let on = s.is_selected(&g.id);
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(6.0)
                .fill(if on { p.accent.alpha(0.18) } else { Color::TRANSPARENT })
                .border(if on { 2.5 } else { 0.0 }, p.accent)
                .child(check_box(p, on).absolute(w - 30.0, 8.0))
        }));
    let sub = if owned {
        "Non installé".to_string()
    } else if s.playing.contains_key(&g.id) {
        "En jeu".to_string()
    } else {
        match s.last_played(g) {
            0 => g.source.label().to_string(),
            t => ago(t),
        }
    };
    col()
        .key(format!("cover-{:016x}", fnv(&g.id)))
        .role("button")
        .label(g.name.clone())
        .w(w)
        .gap(10.0)
        .hover_group()
        .press_scale(0.98)
        .child(art)
        .child(
            col().w(w).gap(3.0)
                .child(text(g.name.clone()).font(ui(13.5, 600.0)).color(if owned { p.muted } else { p.text }).truncate().w(w))
                .child(row().gap(6.0).center_y().child(store_dot(g.source, 6.0)).child(text(sub).font(ui(12.0, 450.0)).color(p.faint).truncate())),
        )
        .on_press_tx(move |s: &mut State, tx: &mut Tx| open_or_select(s, &open_id, tx))
        .on_double_press_tx(move |s: &mut State, tx: &mut Tx| if !s.selecting { s.launch(&play_id, tx) })
        .on_drag(move |s: &mut State, d, tx| s.drag_game(&drag_id, d, tx))
        .on_secondary_press(context_menu(g.id.clone()))
}

/// The small square icon of a game, with its initial behind while it loads.
pub fn game_icon(s: &State, g: &Game, p: &Palette, size: f32) -> El {
    let frame = el().size(size, size).radius(size * 0.24).clip_rounded(size * 0.24);
    match s.icons.get(&g.id) {
        Some(img) if img.is_ready() => frame.pixels(img.pixels()),
        Some(img) if img.error().is_none() => frame.fill(p.text.alpha(0.06)),
        _ => frame.fill(p.text.alpha(0.08)).center().child(
            text(g.name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_default())
                .font(ui(size * 0.42, 650.0))
                .color(p.muted),
        ),
    }
}

/// Right-click on any game: the same menu everywhere (`overlay::game_menu` draws it).
pub fn context_menu(id: String) -> impl Fn(&mut State, rune::PointerPress, &mut Tx) + 'static {
    move |s: &mut State, at, _: &mut Tx| {
        s.overlay = Overlay::GameMenu { id: id.clone(), at: Some((at.x, at.y)) };
    }
}

/// A label and value on one line, for the facts on a game's page.
pub fn fact(p: &Palette, label: &str, value: El) -> El {
    row().w_full().min_h(36.0).py(8.0).gap(16.0).center_y()
        .child(text(label.to_string()).font(ui(13.0, 500.0)).color(p.faint).w(100.0))
        .child(value.grow(1.0).min_w(0.0))
}

pub fn fact_text(p: &Palette, value: impl Into<String>) -> El {
    text(value.into()).font(ui(13.0, 500.0)).color(p.text).wrap()
}

pub fn fact_num(p: &Palette, value: impl Into<String>) -> El {
    text(value.into()).font(num(13.0, 500.0)).color(p.text).truncate()
}

/// Columns of covers that fill `avail` exactly: as many as fit at about `base` wide, then
/// widened to share the leftover.
pub fn cover_columns(avail: f32, base: f32, gap: f32) -> (usize, f32) {
    let cols = (((avail + gap) / (base + gap)).floor() as usize).max(1);
    let w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).floor() - 0.5;
    (cols, w)
}
