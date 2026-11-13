//! Line icons drawn as vector shapes on a 24-unit grid: no icon font, no image decode, and
//! they take the theme's colours like text does.
//!
//! The geometry follows Lucide's conventions (ISC): a 24 grid, a 2-unit stroke, round caps and
//! joins, 1–2 unit corner radii. The stroke scales with the icon, as it would in an SVG, so a
//! 14 px glyph stays as light as a 20 px one instead of thickening into a blot.

use rune::{Color, El, LineCap, Path, Shape, el};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Home,
    Grid,
    List,
    Star,
    StarFilled,
    Settings,
    Search,
    Play,
    Folder,
    Back,
    Refresh,
    More,
    Plus,
    Check,
    Eye,
    Trash,
    ChevronDown,
    ChevronRight,
    Close,
    External,
    Alert,
    Filter,
    Pencil,
    Image,
    Merge,
    /// A filled square: end what runs.
    Stop,
    Download,
}

type Pt = [f32; 2];

/// A 2-unit stroke on the 24 grid, in points; never thinner than a crisp hairline-and-a-bit.
fn stroke_width(size: f32) -> f32 {
    (size / 12.0).max(1.15)
}

pub fn icon(which: Icon, size: f32, c: Color) -> El {
    let w = stroke_width(size);
    // A round dot one stroke wide, in grid units (strokes are in points, fills in grid units).
    let dot = w * 24.0 / size / 2.0;
    el().size(size, size).shape(Pen { s: Shape::new(), w, dot, c }.draw(which).s.view_box(24.0, 24.0))
}

struct Pen {
    s: Shape,
    w: f32,
    dot: f32,
    c: Color,
}

impl Pen {
    fn open(mut self, p: Vec<Pt>) -> Self {
        self.s = self.s.stroke_with(p, false, self.w, LineCap::Round, self.c);
        self
    }
    fn closed(mut self, p: Vec<Pt>) -> Self {
        self.s = self.s.stroke_with(p, true, self.w, LineCap::Round, self.c);
        self
    }
    fn line(self, a: Pt, b: Pt) -> Self {
        self.open(vec![a, b])
    }
    fn path(mut self, p: &Path) -> Self {
        self.s = self.s.stroke_path(p, self.w, LineCap::Round, self.c);
        self
    }
    fn fill(mut self, p: Vec<Pt>) -> Self {
        self.s = self.s.fill(p, self.c);
        self
    }
    fn dot(self, x: f32, y: f32) -> Self {
        let r = self.dot;
        self.fill(arc(x, y, r, 0.0, 360.0, 20))
    }
    fn circle(self, x: f32, y: f32, r: f32) -> Self {
        let mut p = arc(x, y, r, 0.0, 360.0, 48);
        p.pop();
        self.closed(p)
    }

    fn draw(self, which: Icon) -> Self {
        match which {
            Icon::Home => self
                .closed(rounded(&[[3.0, 9.6], [12.0, 2.8], [21.0, 9.6], [21.0, 21.0], [3.0, 21.0]], 2.0, true))
                .open(rounded(&[[9.0, 21.0], [9.0, 13.0], [15.0, 13.0], [15.0, 21.0]], 1.0, false)),
            Icon::Grid => self
                .closed(rrect(3.0, 3.0, 7.0, 7.0, 1.5))
                .closed(rrect(14.0, 3.0, 7.0, 7.0, 1.5))
                .closed(rrect(3.0, 14.0, 7.0, 7.0, 1.5))
                .closed(rrect(14.0, 14.0, 7.0, 7.0, 1.5)),
            Icon::List => self
                .line([8.0, 6.0], [21.0, 6.0])
                .line([8.0, 12.0], [21.0, 12.0])
                .line([8.0, 18.0], [21.0, 18.0])
                .dot(3.5, 6.0)
                .dot(3.5, 12.0)
                .dot(3.5, 18.0),
            Icon::Star => self.closed(rounded(&star(), 1.1, true)),
            Icon::StarFilled => {
                let p = rounded(&star(), 1.1, true);
                self.fill(p.clone()).closed(p)
            }
            // Sliders: three rails, each with a short tick for its knob.
            Icon::Settings => self
                .line([3.0, 5.0], [10.0, 5.0])
                .line([14.0, 5.0], [21.0, 5.0])
                .line([14.0, 2.5], [14.0, 7.5])
                .line([3.0, 12.0], [8.0, 12.0])
                .line([12.0, 12.0], [21.0, 12.0])
                .line([8.0, 9.5], [8.0, 14.5])
                .line([3.0, 19.0], [12.0, 19.0])
                .line([16.0, 19.0], [21.0, 19.0])
                .line([16.0, 16.5], [16.0, 21.5]),
            Icon::Search => self.circle(11.0, 11.0, 7.5).line([16.5, 16.5], [21.0, 21.0]),
            Icon::Play => {
                let p = rounded(&[[6.5, 3.5], [20.0, 12.0], [6.5, 20.5]], 2.2, true);
                self.fill(p.clone()).closed(p)
            }
            Icon::Folder => self.closed(rounded(
                &[[2.0, 4.0], [9.0, 4.0], [11.0, 7.0], [22.0, 7.0], [22.0, 20.0], [2.0, 20.0]],
                1.8,
                true,
            )),
            Icon::Back => self.open(vec![[15.0, 18.0], [9.0, 12.0], [15.0, 6.0]]),
            Icon::Refresh => {
                // rotate-cw: three quarters of a circle, easing into the arrowhead at top right.
                let mut p = arc(12.0, 12.0, 9.0, 0.0, 270.0, 54);
                let tail = Path::new()
                    .move_to([12.0, 3.0])
                    .cubic_to([14.5, 3.0], [16.9, 4.0], [18.7, 5.7])
                    .line_to([21.0, 8.0]);
                p.pop();
                self.open(p).path(&tail).open(vec![[21.0, 3.0], [21.0, 8.0], [16.0, 8.0]])
            }
            Icon::More => {
                let r = self.dot * 1.2;
                self.fill(arc(5.0, 12.0, r, 0.0, 360.0, 20))
                    .fill(arc(12.0, 12.0, r, 0.0, 360.0, 20))
                    .fill(arc(19.0, 12.0, r, 0.0, 360.0, 20))
            }
            Icon::Plus => self.line([12.0, 5.0], [12.0, 19.0]).line([5.0, 12.0], [19.0, 12.0]),
            Icon::Check => self.open(vec![[4.5, 12.5], [9.5, 17.5], [19.5, 6.5]]),
            Icon::Eye => {
                let lid = Path::new()
                    .move_to([2.0, 12.0])
                    .cubic_to([5.5, 5.0], [18.5, 5.0], [22.0, 12.0])
                    .cubic_to([18.5, 19.0], [5.5, 19.0], [2.0, 12.0])
                    .close();
                self.path(&lid).circle(12.0, 12.0, 3.0)
            }
            Icon::Trash => self
                .line([3.0, 6.0], [21.0, 6.0])
                .open(rounded(&[[19.0, 6.0], [19.0, 21.0], [5.0, 21.0], [5.0, 6.0]], 2.0, false))
                .open(rounded(&[[8.0, 6.0], [8.0, 3.0], [16.0, 3.0], [16.0, 6.0]], 1.5, false))
                .line([10.0, 11.0], [10.0, 17.0])
                .line([14.0, 11.0], [14.0, 17.0]),
            Icon::ChevronDown => self.open(vec![[6.0, 9.0], [12.0, 15.0], [18.0, 9.0]]),
            Icon::ChevronRight => self.open(vec![[9.0, 6.0], [15.0, 12.0], [9.0, 18.0]]),
            Icon::Stop => {
                let p = rrect(5.5, 5.5, 13.0, 13.0, 2.5);
                self.fill(p.clone()).closed(p)
            }
            Icon::Close => self.line([6.0, 6.0], [18.0, 18.0]).line([18.0, 6.0], [6.0, 18.0]),
            Icon::External => self
                .open(vec![[15.0, 3.0], [21.0, 3.0], [21.0, 9.0]])
                .line([10.0, 14.0], [21.0, 3.0])
                .open(rounded(&[[18.0, 13.0], [18.0, 21.0], [3.0, 21.0], [3.0, 6.0], [11.0, 6.0]], 2.0, false)),
            // An arrow down into a tray.
            Icon::Download => self
                .line([12.0, 3.0], [12.0, 15.0])
                .open(vec![[7.0, 10.0], [12.0, 15.0], [17.0, 10.0]])
                .open(rounded(&[[3.0, 15.0], [3.0, 21.0], [21.0, 21.0], [21.0, 15.0]], 2.0, false)),
            Icon::Alert => self.circle(12.0, 12.0, 9.5).line([12.0, 7.5], [12.0, 12.5]).dot(12.0, 16.3),
            // A funnel: wide at the top, narrowing to a short stem.
            Icon::Filter => self.closed(rounded(&[[3.0, 4.0], [21.0, 4.0], [14.0, 12.5], [14.0, 20.0], [10.0, 18.0], [10.0, 12.5]], 1.2, true)),
            Icon::Pencil => self
                .closed(rounded(&[[17.0, 3.0], [21.0, 7.0], [8.0, 20.0], [3.0, 21.0], [4.0, 16.0]], 1.2, true))
                .line([14.5, 5.5], [18.5, 9.5]),
            Icon::Image => self
                .closed(rrect(3.0, 3.0, 18.0, 18.0, 2.0))
                .circle(9.0, 9.0, 2.0)
                .open(rounded(&[[21.0, 15.0], [16.0, 10.0], [5.0, 21.0]], 1.0, false)),
            // Two branches joining into one.
            Icon::Merge => self
                .circle(6.0, 5.0, 2.5)
                .circle(18.0, 19.0, 2.5)
                .line([6.0, 7.5], [6.0, 21.0])
                .path(&Path::new().move_to([6.0, 9.0]).cubic_to([6.0, 15.0], [10.0, 19.0], [15.5, 19.0])),
        }
    }
}

/// Points along a circle from `a0` to `a1` degrees (clockwise on screen), both ends included.
fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Pt> {
    (0..=n)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect()
}

fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<Pt> {
    rounded(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]], r, true)
}

/// A polyline whose corners are rounded off by up to `r` (less where a side is too short);
/// an open one keeps its two ends sharp.
fn rounded(p: &[Pt], r: f32, closed: bool) -> Vec<Pt> {
    let n = p.len();
    let mut out = Vec::with_capacity(n * 8);
    for i in 0..n {
        let edge = !closed && (i == 0 || i == n - 1);
        if edge {
            out.push(p[i]);
            continue;
        }
        let (prev, cur, next) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        let (d0, d1) = (dist(prev, cur), dist(cur, next));
        let k = r.min(d0 * 0.45).min(d1 * 0.45);
        let a = lerp(cur, prev, k / d0);
        let b = lerp(cur, next, k / d1);
        for j in 0..=6 {
            let t = j as f32 / 6.0;
            let u = 1.0 - t;
            out.push([
                u * u * a[0] + 2.0 * u * t * cur[0] + t * t * b[0],
                u * u * a[1] + 2.0 * u * t * cur[1] + t * t * b[1],
            ]);
        }
    }
    out
}

fn star() -> Vec<Pt> {
    (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { 9.8 } else { 4.4 };
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            [12.0 + r * a.cos(), 12.9 + r * a.sin()]
        })
        .collect()
}

fn dist(a: Pt, b: Pt) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

fn lerp(a: Pt, b: Pt, t: f32) -> Pt {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
