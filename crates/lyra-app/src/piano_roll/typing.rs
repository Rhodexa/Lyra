//! Typing: edit the piano roll from the keyboard, like code in a text editor.
//! An experiment (2026-11-26) for composing on a laptop with no mouse: it
//! lives in this file so it can be changed or thrown away without touching
//! the mouse editing.
//!
//! **Enter** (or the TYPE button) starts it, **Esc** ends it. There is a caret
//! (a line with a cap on the ruler); the cell after it shows where the next
//! note lands.
//!
//! * **note keys** — type a note at the caret (keys held together: a chord);
//!   the caret moves past it when you let go, and the typed notes stay
//!   selected, ready to fix. **Hold** a key and its notes grow in time with
//!   the song.
//! * **← →** — the caret, one note length · **Ctrl+← →** — to the previous /
//!   next note · **Shift** with either — select across time
//! * **↑ ↓** — the selection a semitone (Ctrl: an octave)
//! * **Alt+← →** — the selection shorter / longer by a snap; its length becomes
//!   the length you type next (as clicking a note does with the mouse)
//! * **Backspace** — the selection, or back a note length and erase what starts
//!   there · **Delete** — the selection, or erase at the caret
//! * **Home / End** — to the start / after the last note
//! * **Ctrl+C X V** — paste lands at the caret, and the caret moves past it
//!
//! The note length is the roll's "length you used last", shared with the
//! mouse; the snap is the finest step (Alt+← →).

use super::{PianoRoll, View, BEAT};
use crate::keys::Action;
use crate::theme::{self, cr, stroke};
use egui::{pos2, vec2, Color32, Painter, Rect};
use lyra_model::{ChannelId, History, Note, Project};
use std::time::Instant;

/// Typing's state, while it's on.
pub(super) struct Typing {
    /// The caret (pattern ticks).
    pub caret: u32,
    /// Shift-selecting: where the selection started.
    anchor: Option<u32>,
    /// Keys held down right now, and the notes they placed (one chord).
    held: Vec<u8>,
    group: Vec<usize>,
    group_start: u32,
    group_len: u32,
    /// When the chord's first key went down, and what Alt+→ added while held.
    since: Instant,
    added: u32,
    /// The caret moved: scroll it (and the selection) into view once.
    pub follow: bool,
}

impl PianoRoll {
    pub fn typing(&self) -> bool {
        self.typing.is_some()
    }

    /// Start typing (the caret at the playhead) or stop.
    pub fn toggle_typing(&mut self) {
        self.typing = match self.typing {
            Some(_) => None,
            None => Some(Typing {
                caret: super::timeline::snap(self.playhead as f32, self.snap).max(0.0) as u32,
                anchor: None,
                held: Vec::new(),
                group: Vec::new(),
                group_start: 0,
                group_len: 0,
                since: Instant::now(),
                added: 0,
                follow: true,
            }),
        };
    }

    /// The finest step: the snap (with snap off, a step).
    fn fine(&self) -> u32 {
        if self.snap <= 1 {
            lyra_model::TICKS_PER_STEP
        } else {
            self.snap
        }
    }

    /// A click on the ruler puts the caret there.
    pub(super) fn typing_seek(&mut self, tick: f32) {
        let snap = self.snap;
        if let Some(t) = &mut self.typing {
            t.caret = super::timeline::snap(tick, snap).max(0.0) as u32;
            t.anchor = None;
        }
    }

    /// A key of the typing keyboard went down or up. The first key down starts
    /// a chord at the caret (more keys while it's held join it); the last key
    /// up moves the caret past it.
    pub fn typing_key(&mut self, on: bool, key: u8, p: &mut Project, h: &mut History, pat: usize, ch: usize) {
        let id = p.channels[ch].id;
        if self.editing != Some((pat, id)) {
            return;
        }
        let (len, velocity) = (self.default_len(), self.last_vel);
        let Some(mut t) = self.typing.take() else { return };
        if on && !t.held.contains(&key) {
            if t.held.is_empty() {
                h.checkpoint(p);
                t.group.clear();
                (t.group_start, t.group_len, t.added, t.anchor) = (t.caret, len, 0, None);
                t.since = Instant::now();
            }
            t.held.push(key);
            let notes = p.patterns[pat].notes_mut(id);
            t.group.push(notes.len());
            notes.push(Note { start: t.group_start, len: t.group_len, key, velocity, link: None });
            self.selection = t.group.clone();
        } else if let Some(i) = t.held.iter().position(|&k| k == key).filter(|_| !on) {
            t.held.remove(i);
            if t.held.is_empty() {
                t.caret = t.group_start + t.group_len;
                t.follow = true;
            }
        }
        self.typing = Some(t);
    }

    /// Every frame while typing: a held chord grows in time with the song:
    /// held as long as a note length lasts, it's that long (never shorter than
    /// the length you type, so a tap is a whole note length).
    pub(super) fn typing_frame(&mut self, bpm: f64, p: &mut Project, pat: usize, id: ChannelId) -> bool {
        let Some(t) = &self.typing else { return false };
        if t.held.is_empty() {
            return false;
        }
        let held = t.since.elapsed().as_secs_f64();
        self.grow_held(held, bpm, p, pat, id);
        true
    }

    pub(super) fn grow_held(&mut self, held_secs: f64, bpm: f64, p: &mut Project, pat: usize, id: ChannelId) {
        let (len, fine) = (self.default_len(), self.fine());
        let Some(t) = &mut self.typing else { return };
        let ticks = held_secs * bpm.max(1.0) / 60.0 * BEAT as f64;
        let grown = ((ticks / fine as f64).floor() as u32 * fine).max(len);
        let want = grown + t.added;
        if want != t.group_len {
            t.group_len = want;
            for &i in &t.group {
                if let Some(n) = p.patterns[pat].notes_mut(id).get_mut(i) {
                    n.len = want;
                }
            }
        }
    }

    /// Keys while typing. Returns true if typing used the key (false: the
    /// roll's own handling goes on, e.g. ↑↓ or Ctrl+C on the selection).
    pub(super) fn typing_action(
        &mut self,
        a: Action,
        p: &mut Project,
        h: &mut History,
        pat: usize,
        id: ChannelId,
    ) -> bool {
        let (len, fine) = (self.default_len(), self.fine());
        let Some(mut t) = self.typing.take() else { return false };
        let used = match a {
            // Alt: lengths. While a chord is held, it grows (or shrinks).
            Action::Arrow { dx, dy: 0, alt: true, .. } if dx != 0 => {
                let d = dx as i64 * fine as i64;
                if !t.held.is_empty() {
                    t.added = (t.added as i64 + d).max(0) as u32;
                } else if !self.selection.is_empty() {
                    h.checkpoint(p);
                    let notes = p.patterns[pat].notes_mut(id);
                    let end = self.selection.iter().filter_map(|&i| notes.get(i)).map(|n| n.start + n.len).max();
                    for &i in &self.selection {
                        if let Some(n) = notes.get_mut(i) {
                            n.len = (n.len as i64 + d).max(fine as i64) as u32;
                        }
                    }
                    let new_end = self.selection.iter().filter_map(|&i| notes.get(i)).map(|n| n.start + n.len).max();
                    if let Some(n) = self.selection.first().and_then(|&i| notes.get(i)) {
                        let l = n.len;
                        self.set_default_len(l); // "use this length from now on"
                    }
                    // Just typed (the caret sits at their end): the caret follows.
                    if end == Some(t.caret) {
                        t.caret = new_end.unwrap_or(t.caret);
                    }
                    t.follow = true;
                }
                true
            }
            // → while holding a chord: a note length more.
            Action::Arrow { dx, dy: 0, .. } if dx != 0 && !t.held.is_empty() => {
                if dx > 0 {
                    t.added += len;
                }
                true
            }
            Action::Arrow { dx, dy: 0, big, shift, .. } if dx != 0 => {
                let notes = p.patterns[pat].notes(id);
                let to = if big {
                    // Ctrl: to the previous / next note start.
                    let starts = notes.iter().map(|n| n.start);
                    if dx > 0 {
                        starts.filter(|&s| s > t.caret).min().unwrap_or(t.caret)
                    } else {
                        starts.filter(|&s| s < t.caret).max().unwrap_or(0)
                    }
                } else if dx > 0 {
                    t.caret + len
                } else {
                    t.caret.saturating_sub(len)
                };
                if shift {
                    let anchor = *t.anchor.get_or_insert(t.caret);
                    let (from, until) = (anchor.min(to), anchor.max(to));
                    self.selection = (0..notes.len()).filter(|&i| (from..until).contains(&notes[i].start)).collect();
                } else {
                    t.anchor = None;
                    self.selection.clear();
                }
                t.caret = to;
                t.follow = true;
                true
            }
            // Up/down: the roll moves the selection; with none, nothing
            // (the arrows mustn't change the channel under you while typing).
            Action::Arrow { dx: 0, .. } => self.selection.is_empty(),
            Action::Backspace | Action::Delete if !self.selection.is_empty() => {
                // Like text: the selection goes, the caret lands where it began.
                let notes = p.patterns[pat].notes(id);
                if let Some(s) = self.selection.iter().filter_map(|&i| notes.get(i)).map(|n| n.start).min() {
                    t.caret = s;
                }
                (t.anchor, t.group) = (None, Vec::new());
                false // the roll deletes them
            }
            Action::Backspace | Action::Delete => {
                if a == Action::Backspace {
                    t.caret = t.caret.saturating_sub(len);
                    t.follow = true;
                }
                let (from, to) = (t.caret, t.caret + len);
                if p.patterns[pat].notes(id).iter().any(|n| (from..to).contains(&n.start)) {
                    h.checkpoint(p);
                    p.patterns[pat].notes_mut(id).retain(|n| !(from..to).contains(&n.start));
                    self.selection.clear();
                    t.group.clear();
                }
                true
            }
            Action::Home | Action::End => {
                t.caret = match a {
                    Action::Home => 0,
                    _ => p.patterns[pat].notes(id).iter().map(|n| n.start + n.len).max().unwrap_or(0),
                };
                (t.anchor, t.follow) = (None, true);
                self.selection.clear();
                true
            }
            // Esc: first the selection goes, then typing ends.
            Action::Deselect if self.selection.is_empty() => {
                self.typing = None;
                return true;
            }
            Action::Deselect => {
                t.anchor = None;
                false
            }
            _ => false,
        };
        self.typing = Some(t);
        used
    }

    /// Keep the caret (and what's selected) in view after it moved.
    pub(super) fn typing_follow(&mut self, grid: Rect, notes: &[Note]) {
        let len = self.default_len();
        let Some(t) = &mut self.typing else { return };
        if !std::mem::take(&mut t.follow) {
            return;
        }
        let x = (t.caret as f32 - self.time.scroll) * self.time.ppt;
        if x < 0.0 || x + len as f32 * self.time.ppt > grid.width() - 24.0 {
            self.time.scroll = (t.caret as f32 - grid.width() * 0.25 / self.time.ppt).max(0.0);
        }
        // Rows: the selected notes (just typed, or moved) stay on screen.
        let rows = grid.height() / self.row_h;
        let keys = self.selection.iter().filter_map(|&i| notes.get(i)).map(|n| n.key as f32);
        let (lo, hi) = keys.fold((f32::MAX, f32::MIN), |(lo, hi), k| (lo.min(k), hi.max(k)));
        if lo <= hi && (hi + 1.0 > self.view_top || lo < self.view_top - rows) {
            self.view_top = ((lo + hi) * 0.5 + rows * 0.5).clamp(rows.min(128.0), 128.0);
        }
    }

    /// Under the notes: the cell the next note lands in (while a chord is
    /// held: the notes growing), and a Shift-selection's span.
    pub(super) fn paint_typing_under(&self, painter: &Painter, v: &View, color: Color32) {
        let Some(t) = &self.typing else { return };
        let g = v.grid;
        let span =
            |from: u32, to: u32| Rect::from_min_max(pos2(v.x(from as f32), g.top()), pos2(v.x(to as f32), g.bottom()));
        if let Some(anchor) = t.anchor {
            let r = span(anchor.min(t.caret), anchor.max(t.caret));
            painter.rect_filled(r.intersect(g), cr(0), theme::TEXT.gamma_multiply(0.06));
        }
        let (from, len) = if t.held.is_empty() { (t.caret, self.default_len()) } else { (t.group_start, t.group_len) };
        painter.rect_filled(span(from, from + len).intersect(g), cr(0), color.gamma_multiply(0.10));
    }

    /// Over the notes: the caret, a line with a cap on the ruler (so it never
    /// reads as the playhead).
    pub(super) fn paint_typing_over(&self, painter: &Painter, v: &View, ruler: Rect, color: Color32) {
        let Some(t) = &self.typing else { return };
        let x = v.x(t.caret as f32);
        if (v.grid.left()..=v.grid.right()).contains(&x) {
            painter.vline(x, ruler.top()..=v.grid.bottom(), stroke(2.0, color));
            painter.rect_filled(Rect::from_min_size(pos2(x - 1.0, ruler.top()), vec2(8.0, 7.0)), cr(1), color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyra_model::TICKS_PER_STEP;

    struct Bench {
        p: Project,
        h: History,
        roll: PianoRoll,
        lead: ChannelId,
    }

    impl Bench {
        fn new() -> Self {
            let mut p = Project::default();
            let lead = p.add_channel("Lead", "lyra.synth", lyra_catalog::default_params("lyra.synth"));
            let mut roll = PianoRoll { editing: Some((0, lead)), ..Default::default() };
            roll.last_len[1] = TICKS_PER_STEP; // type in steps
            roll.toggle_typing();
            Bench { p, h: History::default(), roll, lead }
        }
        fn tap(&mut self, keys: &[u8]) {
            for &k in keys {
                self.roll.typing_key(true, k, &mut self.p, &mut self.h, 0, 0);
            }
            for &k in keys {
                self.roll.typing_key(false, k, &mut self.p, &mut self.h, 0, 0);
            }
        }
        fn act(&mut self, a: Action) {
            self.roll.action(a, &mut self.p, &mut self.h, 0, 0);
        }
        fn arrow(&mut self, dx: i32, dy: i32, big: bool, shift: bool, alt: bool) {
            self.act(Action::Arrow { dx, dy, big, shift, alt });
        }
        fn notes(&self) -> Vec<(u32, u32, u8)> {
            let mut v: Vec<_> = self.p.patterns[0].notes(self.lead).iter().map(|n| (n.start, n.len, n.key)).collect();
            v.sort();
            v
        }
        fn caret(&self) -> u32 {
            self.roll.typing.as_ref().unwrap().caret
        }
    }

    const S: u32 = TICKS_PER_STEP;

    #[test]
    fn typing_a_line_and_fixing_it() {
        let mut b = Bench::new();
        b.tap(&[60]);
        b.tap(&[62]);
        b.tap(&[64, 67]); // a chord
        assert_eq!(b.notes(), vec![(0, S, 60), (S, S, 62), (2 * S, S, 64), (2 * S, S, 67)]);
        assert_eq!(b.caret(), 3 * S);
        assert_eq!(b.roll.selection.len(), 2, "the chord just typed stays selected");
        // Too short: Alt+→ twice; the caret follows, and the next note is that long too.
        b.arrow(1, 0, false, false, true);
        b.arrow(1, 0, false, false, true);
        assert_eq!(b.caret(), 5 * S);
        b.tap(&[65]);
        assert_eq!(b.notes().last(), Some(&(5 * S, 3 * S, 65)));
        // Wrong note: ↑ moves the selection (the roll's own nudge).
        b.arrow(0, 1, false, false, false);
        assert_eq!(b.notes().last(), Some(&(5 * S, 3 * S, 66)));
        // A rest is → (a note length), Backspace takes it back.
        b.arrow(1, 0, false, false, false);
        assert_eq!(b.caret(), 11 * S);
        assert!(b.roll.selection.is_empty(), "moving the caret drops the selection");
    }

    #[test]
    fn moving_selecting_and_erasing() {
        let mut b = Bench::new();
        for k in [60, 62, 64, 65] {
            b.tap(&[k]);
        }
        // Ctrl+←: note to note.
        b.arrow(-1, 0, true, false, false);
        assert_eq!(b.caret(), 3 * S);
        b.arrow(-1, 0, true, false, false);
        assert_eq!(b.caret(), 2 * S);
        // Shift+Ctrl+→: select to the next note and on.
        b.arrow(1, 0, true, true, false);
        b.arrow(1, 0, true, true, false);
        assert_eq!(b.roll.selection.len(), 1, "only the note at 2 steps: the range is [2, 3)");
        b.arrow(1, 0, false, true, false);
        assert_eq!(b.roll.selection.len(), 2);
        // Backspace with a selection: those go, the caret goes where they began.
        b.act(Action::Backspace);
        assert_eq!(b.notes(), vec![(0, S, 60), (S, S, 62)]);
        assert_eq!(b.caret(), 2 * S);
        // Backspace with none: back a length and erase.
        b.act(Action::Backspace);
        assert_eq!(b.notes(), vec![(0, S, 60)]);
        // Home, Delete: erase at the caret, stay.
        b.act(Action::Home);
        b.act(Action::Delete);
        assert!(b.notes().is_empty());
        assert_eq!(b.caret(), 0);
        // Esc twice: the selection, then typing.
        b.tap(&[60]);
        b.act(Action::Deselect);
        assert!(b.roll.typing());
        b.act(Action::Deselect);
        assert!(!b.roll.typing());
    }

    #[test]
    fn holding_grows_in_time_and_paste_lands_at_the_caret() {
        let mut b = Bench::new();
        // 120 bpm: a step is 0.125 s; held 0.4 s, it's 3 steps (never under one).
        b.roll.typing_key(true, 60, &mut b.p, &mut b.h, 0, 0);
        b.roll.grow_held(0.4, 120.0, &mut b.p, 0, b.lead);
        b.roll.typing_key(false, 60, &mut b.p, &mut b.h, 0, 0);
        assert_eq!(b.notes(), vec![(0, 3 * S, 60)]);
        assert_eq!(b.caret(), 3 * S);
        // Ctrl+C the selected (just typed) note, paste: it lands at the caret.
        b.act(Action::Copy);
        b.act(Action::Paste);
        assert_eq!(b.notes(), vec![(0, 3 * S, 60), (3 * S, 3 * S, 60)]);
        assert_eq!(b.caret(), 6 * S);
    }
}
