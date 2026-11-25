//! The screen: the performer thinking. A port of `docs/design/faceplate.html`
//! (`drawLcd` and the functions it calls), drawn from the engine's telemetry
//! instead of the mock's tapes.
//!
//! The LCD is drawn in its own units (640×336) and shown at [`SCALE`]. It
//! cheats: plain text with LCD shadows. Every piece of ink is painted twice,
//! its shadow on the backplane (offset, pale), then itself. Ink shades:
//! 0 unlit (barely there), 1 faint, 2 mid, 3 dark. The screen stays fully
//! lit when nothing plays (Rhodexa: dimming was annoying). Rectangles land
//! on whole pixels, so the 0.68 scale doesn't blur their edges.

use crate::look::{self, galley, put, Align, LCD, LCD_FONT, LCD_SEMI};
use egui::{vec2, Color32, Painter, Pos2, Shape, Stroke};
use octavia_core::performer::{Decision, Played};
use octavia_core::telemetry::{NoteInfo, StringSound};
use octavia_core::Snapshot;

/// The LCD's own size, in its units.
pub const W: f32 = 640.0;
pub const H: f32 = 336.0;
/// How big it's shown on the plate.
pub const SCALE: f32 = 0.68;
/// Text is never smaller than this (units).
const MIN_TEXT: f32 = 11.5;
/// Unlit ink: this far from the glass toward the faint ink.
const GHOST: f32 = 0.15;
/// The ink's shadow: offset (units) and how far from the glass toward mid.
const SHADOW_OFF: f32 = 2.0;
const SHADOW: f32 = 0.5 * 0.55;

/// The glass's layout (units).
struct L;
impl L {
    const PAD: f32 = 14.0;
    const MSG: f32 = 30.0;
    const ANN: f32 = 52.0;
    const RULE: f32 = 60.0;
    const MID: f32 = 68.0;
    const COL_X: f32 = 318.0;
    const DIV: f32 = 108.0;
    const NECK: f32 = 232.0;
}

/// Everything the screen shows, gathered by the plate each frame.
pub struct View {
    pub snap: Snapshot,
    pub violin: bool,
    /// The top line: the player, the control being touched, or PERF OFF.
    pub message: String,
    pub take: u32,
    /// 0 arco, 1 pizz, 2 trem.
    pub patch: usize,
    pub doubles: bool,
    pub performer: bool,
    pub log: bool,
    /// Something sounds or is about to (lights ▶ PLAY).
    pub active: bool,
    /// The hand bracket, eased toward where the hand is.
    pub hand: f32,
    /// A slide's dotted trail: string, from, to (pitches), and 0..1 how faded.
    pub trail: Option<(usize, f32, f32, f32)>,
}

/// The glass, as a painter in its own units.
pub struct Lcd<'a> {
    p: &'a Painter,
    origin: Pos2,
    k: f32,
    /// Physical pixels per point, to land edges on pixels.
    ppp: f32,
    ink: [Color32; 4],
    shadow: Color32,
}

impl<'a> Lcd<'a> {
    /// The glass with its top-left at `origin`, `k` points per unit.
    pub fn new(p: &'a Painter, origin: Pos2, k: f32) -> Self {
        let glass = LCD[0];
        Self {
            p,
            origin,
            k,
            ppp: p.ctx().pixels_per_point(),
            ink: [look::mix(glass, LCD[1], GHOST), LCD[1], LCD[2], LCD[3]],
            shadow: look::mix(glass, LCD[2], SHADOW),
        }
    }

    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y) * self.k
    }

    /// [`Lcd::at`] on the nearest pixel corner.
    fn snapped(&self, x: f32, y: f32) -> Pos2 {
        let p = self.at(x, y);
        let r = |v: f32| (v * self.ppp).round() / self.ppp;
        Pos2::new(r(p.x), r(p.y))
    }

    /// Paints with the shadow first (offset), then the ink.
    fn ink(&self, s: usize, draw: impl Fn(Color32, f32)) {
        if s > 0 {
            draw(self.shadow, SHADOW_OFF);
        }
        draw(self.ink[s], 0.0);
    }

    pub fn rect(&self, s: usize, x: f32, y: f32, w: f32, h: f32) {
        self.ink(s, |c, o| {
            let r = egui::Rect::from_min_max(self.snapped(x + o, y + o), self.snapped(x + o + w, y + o + h));
            self.p.rect_filled(r, 0.0, c);
        });
    }

    /// A convex polygon.
    pub fn poly(&self, s: usize, pts: &[(f32, f32)]) {
        // Clockwise on screen, so the edges are smoothed outward.
        let area: f32 = pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum();
        let mut pts = pts.to_vec();
        if area < 0.0 {
            pts.reverse();
        }
        self.ink(s, |c, o| {
            let pts = pts.iter().map(|&(x, y)| self.at(x + o, y + o)).collect();
            self.p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
        });
    }

    /// Text with its baseline at `y` (units). `track` is letter spacing in units.
    pub fn text(&self, s: usize, text: &str, x: f32, y: f32, size: f32, align: Align, track: f32, semi: bool) {
        let font = if semi { LCD_SEMI } else { LCD_FONT };
        let g = galley(self.p, text, font, size.max(MIN_TEXT) * self.k, track * self.k);
        self.ink(s, |c, o| put(self.p, &g, self.at(x + o, y + o), align, c));
    }

    /// How wide [`Lcd::text`] would be, in units.
    pub fn text_w(&self, text: &str, size: f32, track: f32) -> f32 {
        galley(self.p, text, LCD_FONT, size.max(MIN_TEXT) * self.k, track * self.k).size().x / self.k
    }

    /// Printed lettering: small, tracked.
    fn printed(&self, s: usize, text: &str, x: f32, y: f32, size: f32, align: Align) -> f32 {
        self.text(s, text, x, y, size, align, 1.2, false);
        self.text_w(text, size, 1.2)
    }

    /// Seven-segment digits (the take): `w`×`h` each, unlit segments off.
    fn digits(&self, text: &str, x: f32, y: f32, w: f32, h: f32) {
        let t = (w * 0.22).max(2.0);
        let g = t * 0.28;
        let (xl, xr, yt, yb, ym) = (t / 2.0, w - t / 2.0, t / 2.0, h - t / 2.0, h / 2.0);
        let hs = |x0: f32, x1: f32, yy: f32| {
            let (x0, x1) = (x0 + g, x1 - g);
            [
                (x0, yy),
                (x0 + t / 2.0, yy - t / 2.0),
                (x1 - t / 2.0, yy - t / 2.0),
                (x1, yy),
                (x1 - t / 2.0, yy + t / 2.0),
                (x0 + t / 2.0, yy + t / 2.0),
            ]
        };
        let vs = |xx: f32, y0: f32, y1: f32| {
            let (y0, y1) = (y0 + g, y1 - g);
            [
                (xx, y0),
                (xx + t / 2.0, y0 + t / 2.0),
                (xx + t / 2.0, y1 - t / 2.0),
                (xx, y1),
                (xx - t / 2.0, y1 - t / 2.0),
                (xx - t / 2.0, y0 + t / 2.0),
            ]
        };
        let segs = [
            ('a', hs(xl, xr, yt)),
            ('b', vs(xr, yt, ym)),
            ('c', vs(xr, ym, yb)),
            ('d', hs(xl, xr, yb)),
            ('e', vs(xl, ym, yb)),
            ('f', vs(xl, yt, ym)),
            ('g', hs(xl, xr, ym)),
        ];
        for (i, ch) in text.chars().enumerate() {
            let on = match ch {
                '0' => "abcdef",
                '1' => "bc",
                '2' => "abged",
                '3' => "abcdg",
                '4' => "fgbc",
                '5' => "afgcd",
                '6' => "afedcg",
                '7' => "abc",
                '8' => "abcdefg",
                '9' => "abcdfg",
                _ => "",
            };
            let ox = x + i as f32 * w * 1.35;
            for (name, pts) in &segs {
                if on.contains(*name) {
                    let pts: Vec<_> = pts.iter().map(|&(px, py)| (ox + px, y + py)).collect();
                    self.poly(3, &pts);
                }
            }
        }
    }

    /// The string player's bow marks: ⊓ down-bow, V up-bow (drawn, so any font works).
    fn bow_mark(&self, s: usize, down: bool, x: f32, y: f32, size: f32) {
        let t = (size * 0.16).max(1.5);
        if down {
            self.rect(s, x, y, size, t * 1.6);
            self.rect(s, x, y, t, size);
            self.rect(s, x + size - t, y, t, size);
        } else {
            let tip = (x + size / 2.0, y + size);
            let inner = (x + size / 2.0, y + size - t * 1.6);
            self.poly(s, &[(x, y), (x + t * 1.3, y), inner, tip]);
            self.poly(s, &[(x + size, y), (x + size - t * 1.3, y), inner, tip]);
        }
    }

    /// A BIOS-style meter: an outlined box with a solid bar inside, `fill`
    /// 0..1 of the way. (Blocks were the mock's; at 0.68 they beat against
    /// the pixels, so Rhodexa asked for solid bars.)
    fn box_meter(&self, x: f32, y: f32, w: f32, h: f32, fill: f32) {
        self.rect(2, x, y, w, 2.0);
        self.rect(2, x, y + h - 2.0, w, 2.0);
        self.rect(2, x, y, 2.0, h);
        self.rect(2, x + w - 2.0, y, 2.0, h);
        let fill = fill.clamp(0.0, 1.0);
        if fill > 0.0 {
            self.rect(3, x + 4.0, y + 4.0, (w - 8.0) * fill, h - 8.0);
        }
    }
}

// ───────────────────────────────────────────────────────────── words

const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const ROMAN: [&str; 4] = ["IV", "III", "II", "I"];

fn note_name(pitch: f32) -> String {
    let n = pitch.round() as i32;
    format!("{}{}", NAMES[n.rem_euclid(12) as usize], n.div_euclid(12) - 1)
}

/// "F4", or a double stop "A3+D3" (top first) when it's played as one.
fn notes_name(n: &NoteInfo) -> String {
    let double = n.decision.map_or(true, |d| d.double);
    match n.pitch2 {
        Some(p2) if double => format!("{}+{}", note_name(n.pitch), note_name(p2)),
        _ => note_name(n.pitch),
    }
}

fn how(p: Played) -> &'static str {
    match p {
        Played::Fresh => "FROM SILENCE",
        Played::NewBow => "NEW BOW",
        Played::Slurred => "SLURRED",
        Played::Staccato => "STACCATO",
        Played::Bounced => "BOUNCED",
        Played::Tremolo => "TREMOLO",
        Played::Plucked => "PLUCKED",
    }
}

fn how_short(p: Played) -> &'static str {
    match p {
        Played::Fresh => "START",
        Played::NewBow => "NEW",
        Played::Slurred => "SLUR",
        Played::Staccato => "STAC",
        Played::Bounced => "BOUNCE",
        Played::Tremolo => "TREM",
        Played::Plucked => "PIZZ",
    }
}

fn plucked(d: Option<Decision>) -> bool {
    d.is_some_and(|d| d.played == Played::Plucked)
}

/// A number the way the log prints it: whole when it is whole.
fn num(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i32)
    } else {
        format!("{v:.1}")
    }
}

// ───────────────────────────────────────────────────────────── the glass

/// The whole screen.
pub fn draw(lcd: &Lcd, v: &View) {
    lcd.text(3, &v.message.to_uppercase(), L::PAD, L::MSG, 22.0, Align::Left, 2.0, false);
    // The take: the one place the segments stay.
    let tx = W - L::PAD - 2.0 * 14.0 * 1.35 + 4.0;
    lcd.digits(&format!("{:02}", v.take), tx, L::MSG - 22.0, 14.0, 24.0);
    lcd.printed(2, "TAKE", tx - 8.0, L::MSG, 9.0, Align::Right);
    // Printed words, lit when true, barely there when not.
    let s = &v.snap;
    let grit = s.frame.grit > 0.12 && s.frame.loudness > 0.02;
    let items: [Option<(&str, bool)>; 15] = [
        Some(("CELLO", !v.violin)),
        Some(("VIOLIN", v.violin)),
        None,
        Some(("ARCO", v.patch == 0)),
        Some(("PIZZ", v.patch == 1)),
        Some(("TREM", v.patch == 2)),
        None,
        Some(("DOUBLE", v.doubles)),
        None,
        Some(("GRIT", grit)),
        None,
        Some(("PERF", !v.log)),
        Some(("LOG", v.log)),
        None,
        Some(("PLAY", v.active)),
    ];
    let mut x = L::PAD;
    for it in items {
        let Some((word, lit)) = it else {
            x += 14.0;
            continue;
        };
        let s = if lit { 3 } else { 0 };
        if word == "PLAY" {
            // "▶ PLAY": the triangle drawn, so the font needn't have it.
            lcd.poly(s, &[(x, L::ANN - 8.0), (x + 7.0, L::ANN - 4.0), (x, L::ANN)]);
            x += 11.0;
        }
        lcd.printed(s, word, x, L::ANN, 10.0, Align::Left);
        x += lcd.text_w(word, 10.0, 1.2) + 10.0;
    }
    lcd.rect(2, L::PAD, L::RULE, W - 2.0 * L::PAD, 2.0);
    if v.log {
        log_page(lcd, v);
    } else {
        perf_page(lcd, v);
    }
}

fn perf_page(lcd: &Lcd, v: &View) {
    let cur = v.snap.recent.iter().flatten().last().copied();
    now_panel(lcd, cur.as_ref());
    next_panel(lcd, v, cur.as_ref());
    meters(lcd, v);
    neck_panel(lcd, v, cur.as_ref());
    // Printed rules.
    lcd.rect(2, L::COL_X, L::MID, 2.0, L::NECK - L::MID - 8.0);
    lcd.rect(2, L::PAD, L::DIV, L::COL_X - L::PAD - 12.0, 2.0);
    lcd.rect(2, L::PAD, L::NECK, W - 2.0 * L::PAD, 2.0);
}

/// The note being played, big, and how.
fn now_panel(lcd: &Lcd, cur: Option<&NoteInfo>) {
    let Some(cur) = cur else { return };
    let (x, y) = (L::PAD, L::MID + 30.0);
    lcd.text(3, &notes_name(cur), x, y, 30.0, Align::Left, 0.0, false);
    if let Some(d) = cur.decision {
        lcd.text(3, how(d.played), x + 128.0, y - 4.0, 18.0, Align::Left, 0.5, false);
    }
}

/// What the performer has planned (the look-ahead), and the bow's way now.
fn next_panel(lcd: &Lcd, v: &View, cur: Option<&NoteInfo>) {
    let labels = strings(v.violin);
    let (x, y) = (L::PAD, L::DIV + 20.0);
    lcd.printed(2, "NEXT", x, y, 9.0, Align::Left);
    let up: Vec<&NoteInfo> = v.snap.upcoming.iter().flatten().take(4).collect();
    for (i, n) in up.iter().enumerate() {
        let yy = y + 19.0 + i as f32 * 19.0;
        let s = if i == 0 { 3 } else { 2 };
        lcd.text(s, &notes_name(n), x, yy, 15.0, Align::Left, 0.0, false);
        // Decided: its bow and how. Not yet: where the hand means to play it.
        if let Some(d) = n.decision {
            let mark = if d.played == Played::Plucked { 0 } else { s };
            lcd.bow_mark(mark, d.down, x + 66.0, yy - 10.0, 10.0);
            lcd.text(s, how_short(d.played), x + 82.0, yy, 12.0, Align::Left, 0.5, false);
        }
        let string = n.decision.map(|d| d.string).or(n.intent.map(|i| i.string));
        if let Some(st) = string {
            lcd.text(s, labels[(st as usize).min(3)], x + 150.0, yy, 12.0, Align::Left, 0.0, false);
        }
    }
    if up.is_empty() {
        lcd.text(1, "END", x, y + 19.0, 14.0, Align::Left, 0.0, false);
    }
    // The bow's way now, big, along the bow as the neck draws it: ⊓↓ or V↑.
    let (ax, by) = (204.0, y + 44.0);
    let d = cur.and_then(|c| c.decision);
    let bowing = d.is_some() && !plucked(d) && v.snap.frame.loudness > 0.02;
    let down = d.is_some_and(|d| d.down);
    let (dn, upb) = (if bowing && down { 3 } else { 0 }, if bowing && !down { 3 } else { 0 });
    lcd.bow_mark(dn, true, ax, by - 7.0, 14.0);
    lcd.rect(dn, ax + 22.0, by - 16.0, 6.0, 20.0);
    lcd.poly(dn, &[(ax + 16.0, by + 4.0), (ax + 34.0, by + 4.0), (ax + 25.0, by + 16.0)]);
    lcd.rect(upb, ax + 62.0, by - 4.0, 6.0, 20.0);
    lcd.poly(upb, &[(ax + 56.0, by - 4.0), (ax + 65.0, by - 16.0), (ax + 74.0, by - 4.0)]);
    lcd.bow_mark(upb, false, ax + 80.0, by - 7.0, 14.0);
}

/// The right side: BIOS-style meters.
fn meters(lcd: &Lcd, v: &View) {
    let x = L::COL_X + 12.0;
    let w = W - L::PAD - x;
    let (bx, bw, bh) = (x + 84.0, w - 84.0, 16.0);
    let f = &v.snap.frame;
    let sounding = f.loudness > 0.02;
    let row = |label: &str, y: f32, val: f32| {
        lcd.printed(2, label, x, y + 12.0, 9.0, Align::Left);
        lcd.box_meter(bx, y, bw, bh, val);
    };
    let mut y = L::MID + 6.0;
    row("LOUDNESS", y, f.loudness * 1.5);
    y += 32.0;
    row("BRIGHTNESS", y, if sounding { f.brightness } else { 0.0 });
    // Pitch, like a tuner over ±50 cents: the dark needle is where the note
    // sits, the lighter span how far the vibrato swings.
    y += 32.0;
    lcd.printed(2, "PITCH", x, y + 12.0, 9.0, Align::Left);
    let (ix, iw, needle) = (bx + 4.0, bw - 8.0, 5.0);
    let at = |c: f32| ix + ((c + 50.0) / 100.0).clamp(0.0, 1.0) * (iw - needle);
    lcd.rect(2, bx + bw / 2.0 - 1.0, y - 5.0, 2.0, 4.0);
    lcd.rect(2, bx + bw / 2.0 - 1.0, y + bh + 1.0, 2.0, 4.0);
    match v.snap.tuner {
        Some(t) => {
            lcd.box_meter(bx, y, bw, bh, 0.0);
            let (a, b) = (at(t.lo), at(t.hi));
            lcd.rect(2, a, y + 4.0, b - a + needle, bh - 8.0);
            lcd.rect(3, at(t.cents), y + 4.0, needle, bh - 8.0);
            let r = t.cents.round() as i32;
            let sign = if r > 0 {
                "+"
            } else if r < 0 {
                "−"
            } else {
                ""
            };
            lcd.text(3, &format!("{sign}{}¢", r.abs()), x + 40.0, y + 12.0, 11.0, Align::Left, 0.0, false);
        }
        None => lcd.box_meter(bx, y, bw, bh, 0.0),
    }
    // The player. Full workload at half the engine's scale (as the mock had it).
    let off = !v.performer;
    y += 32.0;
    row("WORKLOAD", y, if off { 0.0 } else { v.snap.workload * 2.0 });
    y += 32.0;
    row("CALM", y, if off { 0.0 } else { v.snap.composure });
    if off {
        lcd.text(3, "PERFORMER OFF", x + w, L::NECK - 8.0, 11.0, Align::Right, 1.0, false);
    }
}

fn strings(violin: bool) -> [&'static str; 4] {
    if violin {
        ["G", "D", "A", "E"]
    } else {
        ["C", "G", "D", "A"]
    }
}

/// Two octaves of the neck.
const SEMIS: f32 = 26.0;

/// The neck, seen from the side, the whole width: nut left, highest string
/// on top, each finger where the engine has it, the hand's reach as a
/// bracket, the bow across the strings by the bridge.
fn neck_panel(lcd: &Lcd, v: &View, cur: Option<&NoteInfo>) {
    let inst = if v.violin { octavia_core::instrument::violin() } else { octavia_core::instrument::cello() };
    let (open, span) = (inst.open_strings, inst.hand_span);
    let labels = strings(v.violin);
    let nut = L::PAD + 22.0;
    let end = W - L::PAD - 70.0;
    let bridge = W - L::PAD - 40.0;
    let sy = |s: usize| L::NECK + 26.0 + (3 - s) as f32 * 16.0;
    let sx = |semi: f32| nut + 6.0 + semi.min(SEMIS) * (end - nut - 6.0) / SEMIS;
    let st = &v.snap.strings;
    // Printed: the nut, the bridge, a mark per semitone, the octaves doubled.
    lcd.rect(2, nut - 4.0, sy(3) - 6.0, 5.0, sy(0) - sy(3) + 12.0);
    lcd.rect(1, bridge - 1.0, sy(3) - 6.0, 2.0, sy(0) - sy(3) + 12.0);
    for k in 1..=SEMIS as usize {
        let big = k % 12 == 0;
        lcd.rect(if big { 2 } else { 1 }, sx(k as f32) - 1.0, sy(0) + 8.0, 2.0, if big { 6.0 } else { 3.0 });
    }
    lcd.printed(2, "8va", sx(12.0), sy(0) + 24.0, 8.0, Align::Center);
    lcd.printed(2, "15ma", sx(24.0), sy(0) + 24.0, 8.0, Align::Center);
    // The strings: played dark and thick, ringing grey, idle faint.
    for (s, state) in st.iter().enumerate() {
        let shade = match state.sound {
            StringSound::Bowed | StringSound::Plucked => 3,
            StringSound::Ringing(l) if l > 0.04 => 2,
            _ => 1,
        };
        let w = if s < 2 { 3.0 } else { 2.0 } + if shade == 3 { 1.0 } else { 0.0 };
        lcd.rect(shade, nut, sy(s) - w / 2.0, bridge + 14.0 - nut, w);
        lcd.text(if shade == 1 { 2 } else { 3 }, labels[s], L::PAD, sy(s) + 5.0, 13.0, Align::Left, 0.0, false);
    }
    // The hand's reach, sliding on shifts.
    let (hx0, hx1, hy) = (sx(v.hand), sx(v.hand + span), sy(3) - 14.0);
    lcd.rect(2, hx0, hy, hx1 - hx0, 2.0);
    lcd.rect(2, hx0, hy, 2.0, 5.0);
    lcd.rect(2, hx1 - 2.0, hy, 2.0, 5.0);
    // A slide's trail, fading after it lands.
    if let Some((s, from, to, age)) = v.trail {
        let (xa, xb) = (sx(from - open[s]), sx(to - open[s]));
        let every = if age < 0.4 {
            1
        } else if age < 0.7 {
            2
        } else {
            4
        };
        let mut x = xa.min(xb);
        let mut k = 0;
        while x <= xa.max(xb) {
            if k % every == 0 {
                lcd.rect(2, x - 1.5, sy(s) - 3.0, 3.0, 6.0);
            }
            x += 4.0;
            k += 1;
        }
    }
    // The fingers: a filled square wherever one is down, played or only held
    // (Rhodexa: one mark, no hollow squares, no tick for open strings: a dark
    // string with no finger is played open).
    let mut beyond = false;
    for (s, state) in st.iter().enumerate() {
        let Some(p) = state.finger else { continue };
        let semi = p - open[s];
        if semi < 0.5 {
            continue;
        }
        if semi > SEMIS + 0.5 {
            beyond = true;
        }
        lcd.rect(3, sx(semi) - 6.0, sy(s) - 6.0, 12.0, 12.0);
    }
    // Notes the neck can't show: below the lowest open string, or above its
    // two octaves. Printed words with arrows under the neck, lit when true.
    let sounding = cur.filter(|n| v.snap.time < n.start + n.len);
    let low = sounding.is_some_and(|n| n.pitch.min(n.pitch2.unwrap_or(n.pitch)) < open[0] - 0.5);
    let high = beyond || sounding.is_some_and(|n| n.pitch > open[3] + SEMIS + 0.5);
    let (wy, lo, hi) = (sy(0) + 24.0, if low { 3 } else { 0 }, if high { 3 } else { 0 });
    lcd.poly(lo, &[(nut - 4.0, wy - 4.0), (nut + 3.0, wy - 8.0), (nut + 3.0, wy)]);
    lcd.printed(lo, "TOO LOW", nut + 8.0, wy, 8.0, Align::Left);
    let w = lcd.printed(hi, "TOO HIGH", end + 4.0, wy, 8.0, Align::Left);
    let ax = end + 4.0 + w + 2.0;
    lcd.poly(hi, &[(ax, wy - 8.0), (ax + 7.0, wy - 4.0), (ax, wy)]);
    // The bow across the strings, or a pluck.
    if st.iter().any(|s| s.sound == StringSound::Bowed) {
        lcd.rect(3, bridge + 6.0, sy(3) - 9.0, 5.0, sy(0) - sy(3) + 18.0);
    } else if let Some(s) = st.iter().position(|s| s.sound == StringSound::Plucked) {
        let y = sy(s);
        lcd.poly(3, &[(bridge + 4.0, y - 7.0), (bridge + 14.0, y), (bridge + 4.0, y + 7.0)]);
    }
}

/// The decisions as a table, newest at the bottom and darkest.
fn log_page(lcd: &Lcd, v: &View) {
    let cols = [
        ("TIME", 0.0),
        ("NOTE", 50.0),
        ("BOW", 110.0),
        ("HOW", 142.0),
        ("STRING", 256.0),
        ("HAND", 318.0),
        ("LOAD", 368.0),
        ("CALM", 418.0),
        ("SHIFT", 470.0),
    ];
    let (x, y) = (L::PAD, L::MID + 12.0);
    for (h, dx) in cols {
        lcd.printed(2, h, x + dx, y, 9.0, Align::Left);
    }
    lcd.rect(2, L::PAD, y + 5.0, W - 2.0 * L::PAD, 2.0);
    const ROWS: usize = 10;
    let done: Vec<&NoteInfo> = v.snap.recent.iter().flatten().filter(|n| n.decision.is_some()).collect();
    let done = &done[done.len().saturating_sub(ROWS)..];
    for (i, n) in done.iter().enumerate() {
        let Some(d) = n.decision else { continue };
        let s = if i == done.len() - 1 { 3 } else { 2 };
        let yy = y + 24.0 + (ROWS - done.len() + i) as f32 * 19.0;
        let string = format!("{}{}", ROMAN[(d.string as usize).min(3)], if n.open { " open" } else { "" });
        let shift = if d.shift.abs() < 0.05 {
            String::new()
        } else {
            format!("{}{}", if d.shift > 0.0 { "+" } else { "−" }, num(d.shift.abs()))
        };
        let cells = [
            format!("{:.2}", n.start),
            notes_name(n),
            String::new(),
            how(d.played).to_string(),
            string,
            num(d.hand),
            format!("{}", (d.workload * 100.0).round() as i32),
            format!("{}", (d.composure * 100.0).round() as i32),
            shift,
        ];
        for (j, c) in cells.iter().enumerate() {
            lcd.text(s, c, x + cols[j].1, yy, 14.0, Align::Left, 0.0, !(j == 1 || j == 3));
        }
        if d.played != Played::Plucked {
            lcd.bow_mark(s, d.down, x + cols[2].1 + 4.0, yy - 10.0, 10.0);
        }
    }
    if done.is_empty() {
        lcd.text(1, "NO NOTES YET", x, y + 60.0, 14.0, Align::Left, 0.0, false);
    }
}

/// Where the glass sits: the screen units → points.
pub fn size() -> egui::Vec2 {
    vec2((W * SCALE).round(), (H * SCALE).round())
}
