//! Chrysalis's **table studio**: make your own wavetable. It is a page of
//! Chrysalis's own editor (the header's TABLE key), not another window.
//!
//! ```text
//!  ┌ name ─ NEW: sine saw square playing ─── PLAY ON: osc a | osc b · LIVE · SAVE ┐
//!  ├ undo · + frame · delete · morph (crossfade | spectral) · normalize · smooth · frames 8…256 ┤
//!  │ the frame, drawn freehand                         │ its first 64 harmonics   │
//!  ├ every frame, side by side (click or drag to pick one) ──────────────────────┤
//! ```
//!
//! It opens with a copy of the table an oscillator plays. Your table is a WAV
//! (frames of 2048 samples, one after another) in `<data dir>/tables/<name>.wav`;
//! saving makes it the channel's custom table ("Imported" in either
//! oscillator's list) and the chosen oscillator plays it, so nothing new is
//! stored in the synth. With LIVE on, every finished stroke or tool saves at
//! once. The studio keeps its own undo (the table isn't part of the project's).

use super::*;
use lyra_chrysalis::table::FRAME;
use std::f32::consts::TAU;
use std::sync::Mutex;

/// What the studio is editing (one per channel, while its window is open).
pub(super) struct Doc {
    osc: usize,
    frames: Vec<Vec<f32>>,
    sel: usize,
    name: String,
    /// Crossfade (0) or spectral (1) morphing.
    morph: usize,
    live: bool,
    /// The last point of the stroke being drawn (sample index, value).
    stroke: Option<(usize, f32)>,
    undo: Vec<Vec<Vec<f32>>>,
    /// Edited since the last save.
    dirty: bool,
    note: Option<String>,
}

type Shared = Arc<Mutex<Doc>>;

fn doc_id(ch: usize) -> Id {
    Id::new(("chrys-studio", ch))
}

/// Open the studio on oscillator `o`'s table.
pub(super) fn open(ui: &Ui, ed: &Ed, o: usize) {
    let t = table_of(ui, ed, o);
    let frames: Vec<Vec<f32>> = (0..t.frames()).map(|f| t.frame(0, f)[..FRAME].to_vec()).collect();
    let name = format!("{} table", ed.p.channels[ed.ch].name);
    let doc = Doc {
        osc: o,
        frames,
        sel: 0,
        name,
        morph: 0,
        live: true,
        stroke: None,
        undo: Vec::new(),
        dirty: false,
        note: None,
    };
    let shared: Shared = Arc::new(Mutex::new(doc));
    ui.ctx().data_mut(|d| d.insert_temp(doc_id(ed.ch), Some(shared)));
}

/// Open the studio on whatever plays now, unless it already holds a table
/// (from the oscillator playing the custom table, else oscillator A).
pub(super) fn ensure(ui: &Ui, ed: &Ed) {
    if ui.ctx().data(|d| d.get_temp::<Option<Shared>>(doc_id(ed.ch))).flatten().is_none() {
        let o = (0..2).find(|&o| ed.choice(p::osc(o, p::TABLE)) == IMPORTED).unwrap_or(0);
        open(ui, ed, o);
    }
}

/// The TABLE page, filling `r` below the editor's header.
pub(super) fn page(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    ensure(ui, ed);
    let Some(shared) = ui.ctx().data(|d| d.get_temp::<Option<Shared>>(doc_id(ed.ch))).flatten() else { return };
    let inner = panel(ui, r, "", TURQ);
    let mut doc = shared.lock().unwrap_or_else(|e| e.into_inner());
    studio(ui, ed, &mut doc, inner);
}

fn studio(ui: &mut Ui, ed: &mut Ed, doc: &mut Doc, all: Rect) {
    let [bar1, bar2, main, strip] = flex::col(all, [Px(KEY_H), Px(KEY_H), Grow(1.0), Px(96.0)], 10.0);
    let color = OSC_COLORS[doc.osc];
    let mut changed = false; // edited by a click (saved now if live)

    // ── Name, new from…, live, save
    let [name_r, new_cap, new_r, _, note_r, on_cap, on_r, live_r, save_r] = flex::row(
        bar1,
        [Px(230.0), Px(36.0), Px(300.0), Px(8.0), Grow(1.0), Px(58.0), Px(150.0), Px(64.0), Px(86.0)],
        6.0,
    );
    caps(ui.painter(), on_cap.right_center(), "play on", TEXT_FAINT, Align2::RIGHT_CENTER);
    let hint = "The oscillator that plays this table when it's saved (the other can pick it too: \"Imported\")";
    let target =
        place(ui, on_r, "studio-target", true, |ui| seg(ui, &["osc a", "osc b"], doc.osc, color, on_r.size(), hint));
    if let Some(k) = target {
        if k != doc.osc {
            doc.osc = k;
            changed = true;
        }
    }
    ui.painter().rect_filled(name_r, cr(3), SCREEN);
    let mut name_ui = ui.new_child(UiBuilder::new().max_rect(name_r.shrink2(vec2(8.0, 3.0))));
    name_ui.add(
        egui::TextEdit::singleline(&mut doc.name)
            .frame(false)
            .text_color(TEXT)
            .desired_width(name_r.width() - 16.0)
            .hint_text("Table name"),
    );
    caps(ui.painter(), new_cap.right_center(), "new", TEXT_FAINT, Align2::RIGHT_CENTER);
    let hint = "Start over from a sine, a saw, a square, or the table the oscillator plays now";
    let fresh = place(ui, new_r, "studio-new", true, |ui| {
        seg(ui, &["sine", "saw", "square", "playing"], usize::MAX, color, new_r.size(), hint)
    });
    if let Some(k) = fresh {
        push_undo(doc);
        doc.frames = match k {
            0 => vec![shape(|t| (TAU * t).sin())],
            1 => vec![shape(|t| 1.0 - 2.0 * t)],
            2 => vec![shape(|t| if t < 0.5 { 1.0 } else { -1.0 })],
            _ => {
                let t = table_of(ui, ed, doc.osc);
                (0..t.frames()).map(|f| t.frame(0, f)[..FRAME].to_vec()).collect()
            }
        };
        doc.sel = 0;
        changed = true;
    }
    if let Some(note) = &doc.note {
        // Clipped to its gap: never over the keys beside it.
        let p = ui.painter_at(note_r);
        p.text(note_r.left_center() + vec2(4.0, 0.0), Align2::LEFT_CENTER, note, FontId::proportional(11.0), TEXT_DIM);
    }
    let hint = "Live: every stroke and tool is saved and heard at once";
    if place(ui, live_r, "studio-live", true, |ui| key(ui, "live", live_r.size(), doc.live, color, hint).clicked()) {
        doc.live = !doc.live;
    }
    let hint = "Save the table and play it on this oscillator";
    if place(ui, save_r, "studio-save", true, |ui| key(ui, "save", save_r.size(), doc.dirty, color, hint).clicked()) {
        save(ui, ed, doc);
    }

    // ── Tools
    let [undo_r, add_r, del_r, _, morph_style, morph_r, _, norm_r, smooth_r, _, res_cap, res_r] = flex::row(
        bar2,
        [
            Px(62.0),
            Px(80.0),
            Px(70.0),
            Px(10.0),
            Px(170.0),
            Px(70.0),
            Px(10.0),
            Px(92.0),
            Px(74.0),
            Grow(1.0),
            Px(52.0),
            Px(240.0),
        ],
        4.0,
    );
    let n = doc.frames.len();
    if place(ui, undo_r, "studio-undo", !doc.undo.is_empty(), |ui| {
        key(ui, "undo", undo_r.size(), false, color, "Undo the last change in the studio").clicked()
    }) {
        if let Some(f) = doc.undo.pop() {
            doc.frames = f;
            doc.sel = doc.sel.min(doc.frames.len() - 1);
            changed = true;
        }
    }
    let full = n >= lyra_chrysalis::table::MAX_FRAMES;
    if place(ui, add_r, "studio-add", !full, |ui| {
        key(ui, "+ frame", add_r.size(), false, color, "A copy of this frame, right after it").clicked()
    }) {
        push_undo(doc);
        let copy = doc.frames[doc.sel].clone();
        doc.frames.insert(doc.sel + 1, copy);
        doc.sel += 1;
        changed = true;
    }
    if place(ui, del_r, "studio-del", n > 1, |ui| {
        key(ui, "delete", del_r.size(), false, color, "Remove this frame").clicked()
    }) {
        push_undo(doc);
        doc.frames.remove(doc.sel);
        doc.sel = doc.sel.min(doc.frames.len() - 1);
        changed = true;
    }
    let hint = "Morphing: crossfade the waves, or move their harmonics (spectral: a formant glides instead of fading)";
    let pick = place(ui, morph_style, "studio-morph-style", true, |ui| {
        seg(ui, &["crossfade", "spectral"], doc.morph, color, morph_style.size(), hint)
    });
    if let Some(k) = pick {
        doc.morph = k;
    }
    let hint = "Fill every frame between the first and the last with a morph from one to the other";
    if place(ui, morph_r, "studio-morph", n > 2, |ui| key(ui, "morph", morph_r.size(), false, color, hint).clicked()) {
        push_undo(doc);
        morph(&mut doc.frames, doc.morph == 1);
        changed = true;
    }
    if place(ui, norm_r, "studio-norm", true, |ui| {
        key(ui, "normalize", norm_r.size(), false, color, "Make the loudest frame peak at full scale").clicked()
    }) {
        push_undo(doc);
        normalize(&mut doc.frames);
        changed = true;
    }
    if place(ui, smooth_r, "studio-smooth", true, |ui| {
        key(ui, "smooth", smooth_r.size(), false, color, "Soften this frame's edges").clicked()
    }) {
        push_undo(doc);
        smooth(&mut doc.frames[doc.sel]);
        changed = true;
    }
    caps(ui.painter(), res_cap.right_center(), "frames", TEXT_FAINT, Align2::RIGHT_CENTER);
    const SIZES: [usize; 6] = [8, 16, 32, 64, 128, 256];
    let at = SIZES.iter().position(|&s| s == n).unwrap_or(usize::MAX);
    let hint = "Resize the table: its frames are stretched (each new one a mix of its neighbours)";
    let pick = place(ui, res_r, "studio-resize", true, |ui| {
        seg(ui, &["8", "16", "32", "64", "128", "256"], at, color, res_r.size(), hint)
    });
    if let Some(k) = pick {
        push_undo(doc);
        doc.frames = resize(&doc.frames, SIZES[k]);
        doc.sel = doc.sel * SIZES[k] / n.max(1);
        doc.sel = doc.sel.min(SIZES[k] - 1);
        changed = true;
    }

    // ── The frame, and its harmonics
    let [canvas_r, harm_r] = flex::row(main, [Grow(1.0), Px(300.0)], 10.0);
    let drew = canvas(ui, doc, canvas_r, color);
    let tuned = harmonics(ui, doc, harm_r, color);
    // ── Every frame
    frame_strip(ui, doc, strip, color);

    if changed || drew || tuned {
        doc.dirty = true;
        if doc.live {
            save(ui, ed, doc);
        }
    }
}

fn shape(f: impl Fn(f32) -> f32) -> Vec<f32> {
    (0..FRAME).map(|i| f(i as f32 / FRAME as f32)).collect()
}

fn push_undo(doc: &mut Doc) {
    doc.undo.push(doc.frames.clone());
    if doc.undo.len() > 40 {
        doc.undo.remove(0);
    }
}

/// Draw the selected frame freehand; returns true when a stroke ends.
fn canvas(ui: &mut Ui, doc: &mut Doc, r: Rect, color: Color32) -> bool {
    let resp = ui.interact(r, Id::new("studio-canvas"), Sense::click_and_drag());
    let inner = r.shrink2(vec2(12.0, 12.0));
    let to_index =
        |x: f32| (((x - inner.left()) / inner.width()) * FRAME as f32).clamp(0.0, (FRAME - 1) as f32) as usize;
    let to_value = |y: f32| (1.0 - 2.0 * (y - inner.top()) / inner.height()).clamp(-1.0, 1.0);
    let mut ended = false;
    if resp.drag_started() || resp.clicked() {
        push_undo(doc);
    }
    if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
        let (i, v) = (to_index(pos.x), to_value(pos.y));
        let (i0, v0) = doc.stroke.unwrap_or((i, v));
        // Fill every sample between the last point and this one, so a fast stroke leaves no gaps.
        let (a, b) = if i0 <= i { ((i0, v0), (i, v)) } else { ((i, v), (i0, v0)) };
        let frame = &mut doc.frames[doc.sel];
        for (k, s) in frame.iter_mut().enumerate().take(b.0 + 1).skip(a.0) {
            let t = if b.0 > a.0 { (k - a.0) as f32 / (b.0 - a.0) as f32 } else { 1.0 };
            *s = a.1 + (b.1 - a.1) * t;
        }
        doc.stroke = Some((i, v));
    }
    if resp.drag_stopped() || resp.clicked() {
        doc.stroke = None;
        ended = true;
    }
    if let Some(step) = wheel_step(ui, r) {
        doc.sel = (doc.sel as i32 + step).clamp(0, doc.frames.len() as i32 - 1) as usize;
    }
    resp.on_hover_text("Draw the wave · scroll to change frame");

    let p = ui.painter_at(r);
    screen(&p, r);
    p.line_segment([pos2(inner.left(), inner.center().y), pos2(inner.right(), inner.center().y)], stroke(1.0, GRID));
    for k in 1..4 {
        let x = inner.left() + inner.width() * k as f32 / 4.0;
        p.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(1.0, alpha(GRID, 0.5)));
    }
    let wave = |f: &[f32]| -> Vec<Pos2> {
        (0..=400)
            .map(|k| {
                let i = (k * (FRAME - 1) / 400).min(FRAME - 1);
                pos2(
                    inner.left() + inner.width() * k as f32 / 400.0,
                    inner.center().y - f[i].clamp(-1.0, 1.0) * inner.height() * 0.5,
                )
            })
            .collect()
    };
    // Its neighbours, faint, so you can see where it goes.
    for d in [-1i32, 1] {
        let k = doc.sel as i32 + d;
        if (0..doc.frames.len() as i32).contains(&k) {
            p.add(Shape::line(wave(&doc.frames[k as usize]), stroke(1.0, alpha(color, 0.25))));
        }
    }
    let pts = wave(&doc.frames[doc.sel]);
    glow_fill(&p, &pts, inner.center().y, color);
    glow_line(&p, pts, color, 1.8);
    caps(
        &p,
        r.right_bottom() - vec2(10.0, 9.0),
        &format!("{} / {}", doc.sel + 1, doc.frames.len()),
        TEXT_DIM,
        Align2::RIGHT_CENTER,
    );
    ended
}

/// The frame's spectrum: (amplitude, phase) of harmonics 1.. up to `n`.
fn spectrum(frame: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let (mut re, mut im) = (frame.to_vec(), vec![0.0; FRAME]);
    lyra_dsp::fft(&mut re, &mut im, false);
    (re, im)
}

const HARM: usize = 64;

/// The first 64 harmonics as bars; drag across them to set their levels
/// (right-click: silence one). Returns true when an edit ends.
fn harmonics(ui: &mut Ui, doc: &mut Doc, r: Rect, color: Color32) -> bool {
    let resp = ui.interact(r, Id::new("studio-harmonics"), Sense::click_and_drag());
    let inner = r.shrink2(vec2(10.0, 10.0));
    let bar_w = inner.width() / HARM as f32;
    let (mut re, mut im) = spectrum(&doc.frames[doc.sel]);
    let amp = |re: &[f32], im: &[f32], h: usize| 2.0 * re[h].hypot(im[h]) / FRAME as f32;
    let mut ended = false;
    if resp.drag_started() || resp.clicked() || resp.secondary_clicked() {
        push_undo(doc);
    }
    if let Some(pos) = resp.interact_pointer_pos() {
        let h = ((pos.x - inner.left()) / bar_w).floor() as i64 + 1;
        let editing = resp.dragged() || resp.clicked() || resp.secondary_clicked();
        if editing && (1..=HARM as i64).contains(&h) {
            let h = h as usize;
            let a = if resp.secondary_clicked() {
                0.0
            } else {
                ((inner.bottom() - pos.y) / inner.height()).clamp(0.0, 1.0)
            };
            let old = amp(&re, &im, h);
            // Keep the harmonic's phase; a new one starts as a sine.
            let (cr_, ci) = if old > 1e-6 { (re[h] / old, im[h] / old) } else { (0.0, -1.0) };
            let target = a * FRAME as f32 / 2.0;
            re[h] = cr_ * target;
            im[h] = ci * target;
            re[FRAME - h] = re[h];
            im[FRAME - h] = -im[h];
            let (mut r2, mut i2) = (re.clone(), im.clone());
            lyra_dsp::fft(&mut r2, &mut i2, true);
            doc.frames[doc.sel] = r2.iter().map(|x| x / FRAME as f32).collect();
        }
    }
    if resp.drag_stopped() || resp.clicked() || resp.secondary_clicked() {
        ended = true;
    }
    resp.on_hover_text("Harmonics 1–64: drag to set their levels · right-click: silence one");

    let p = ui.painter_at(r);
    screen(&p, r);
    let (re, im) = spectrum(&doc.frames[doc.sel]);
    for h in 1..=HARM {
        let a = amp(&re, &im, h).min(1.0);
        let x0 = inner.left() + (h - 1) as f32 * bar_w;
        let bar = Rect::from_min_max(
            pos2(x0 + 0.5, inner.bottom() - a * inner.height()),
            pos2(x0 + bar_w - 0.5, inner.bottom()),
        );
        if a > 0.002 {
            p.rect_filled(bar.expand2(vec2(0.0, 2.0)), cr(1), alpha(color, 0.12));
            p.rect_filled(bar, cr(1), alpha(color, 0.85));
        }
        if h == 1 || h % 8 == 0 {
            p.text(
                pos2(x0 + bar_w * 0.5, inner.bottom() + 1.0),
                Align2::CENTER_TOP,
                h.to_string(),
                FontId::proportional(8.5),
                TEXT_FAINT,
            );
        }
    }
    ended
}

/// Every frame side by side: click or drag along it to pick one.
fn frame_strip(ui: &mut Ui, doc: &mut Doc, r: Rect, color: Color32) {
    let resp = ui.interact(r, Id::new("studio-strip"), Sense::click_and_drag());
    let n = doc.frames.len();
    let w = r.width() / n as f32;
    if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.dragged()) {
        doc.sel = (((pos.x - r.left()) / w) as usize).min(n - 1);
    }
    resp.on_hover_text("Every frame: click or drag to pick one");
    let p = ui.painter_at(r);
    screen(&p, r);
    for (k, f) in doc.frames.iter().enumerate() {
        let cell =
            Rect::from_min_size(pos2(r.left() + k as f32 * w, r.top()), vec2(w, r.height())).shrink2(vec2(0.5, 6.0));
        let sel = k == doc.sel;
        if sel {
            p.rect_filled(cell.expand2(vec2(0.5, 6.0)), cr(2), alpha(color, 0.18));
        }
        let steps = (cell.width() as usize).clamp(2, 48);
        let pts: Vec<Pos2> = (0..=steps)
            .map(|s| {
                let i = (s * (FRAME - 1) / steps).min(FRAME - 1);
                pos2(
                    cell.left() + cell.width() * s as f32 / steps as f32,
                    cell.center().y - f[i].clamp(-1.0, 1.0) * cell.height() * 0.45,
                )
            })
            .collect();
        p.add(Shape::line(pts, stroke(if sel { 1.4 } else { 1.0 }, if sel { color } else { alpha(color, 0.5) })));
    }
}

/// Every frame between the first and the last, from one to the other.
fn morph(frames: &mut [Vec<f32>], spectral: bool) {
    let n = frames.len();
    if n < 3 {
        return;
    }
    let (a, b) = (frames[0].clone(), frames[n - 1].clone());
    if !spectral {
        for (k, f) in frames.iter_mut().enumerate().take(n - 1).skip(1) {
            let t = k as f32 / (n - 1) as f32;
            for (i, s) in f.iter_mut().enumerate() {
                *s = a[i] + (b[i] - a[i]) * t;
            }
        }
        return;
    }
    // Spectral: each harmonic's level and phase glide from one to the other.
    let (ar, ai) = spectrum(&a);
    let (br, bi) = spectrum(&b);
    for (k, f) in frames.iter_mut().enumerate().take(n - 1).skip(1) {
        let t = k as f32 / (n - 1) as f32;
        let (mut re, mut im) = (vec![0.0; FRAME], vec![0.0; FRAME]);
        for h in 1..FRAME / 2 {
            let (ma, mb) = (ar[h].hypot(ai[h]), br[h].hypot(bi[h]));
            let (pa, mut pb) = (ai[h].atan2(ar[h]), bi[h].atan2(br[h]));
            // The short way round.
            while pb - pa > PI {
                pb -= TAU;
            }
            while pa - pb > PI {
                pb += TAU;
            }
            let (m, ph) = (ma + (mb - ma) * t, pa + (pb - pa) * t);
            re[h] = m * ph.cos();
            im[h] = m * ph.sin();
            re[FRAME - h] = re[h];
            im[FRAME - h] = -im[h];
        }
        lyra_dsp::fft(&mut re, &mut im, true);
        *f = re.iter().map(|x| x / FRAME as f32).collect();
    }
}

fn normalize(frames: &mut [Vec<f32>]) {
    let peak = frames.iter().flatten().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 1e-6 {
        for x in frames.iter_mut().flatten() {
            *x /= peak;
        }
    }
}

/// A gentle low-pass that wraps around the cycle (a frame is a loop).
fn smooth(frame: &mut [f32]) {
    for _ in 0..3 {
        let prev = frame.to_vec();
        let n = prev.len();
        for i in 0..n {
            frame[i] = 0.25 * prev[(i + n - 1) % n] + 0.5 * prev[i] + 0.25 * prev[(i + 1) % n];
        }
    }
}

/// `count` frames from `frames`, each a mix of its two nearest originals.
fn resize(frames: &[Vec<f32>], count: usize) -> Vec<Vec<f32>> {
    let n = frames.len();
    (0..count)
        .map(|k| {
            let x = if count > 1 { k as f32 * (n - 1) as f32 / (count - 1) as f32 } else { 0.0 };
            let (i, t) = (x as usize, x.fract());
            let (a, b) = (&frames[i.min(n - 1)], &frames[(i + 1).min(n - 1)]);
            a.iter().zip(b).map(|(p, q)| p + (q - p) * t).collect()
        })
        .collect()
}

/// A table's file name from its name.
fn file_name(name: &str) -> String {
    let clean: String =
        name.chars().map(|c| if c.is_alphanumeric() || " -_.()'&+".contains(c) { c } else { '_' }).collect();
    let clean = clean.trim().trim_start_matches('.');
    format!("{}.wav", if clean.is_empty() { "table" } else { clean })
}

/// Write the table as a WAV, make it this channel's sample, and play it on
/// the oscillator (the engine reads the file again).
fn save(ui: &Ui, ed: &mut Ed, doc: &mut Doc) {
    let dir = crate::files::data_dir().join("tables");
    let path = dir.join(file_name(&doc.name));
    let write = || -> Result<(), String> {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let tmp = path.with_extension("tmp");
        let mut w = hound::WavWriter::create(&tmp, spec).map_err(|e| e.to_string())?;
        for s in doc.frames.iter().flatten() {
            w.write_sample(*s).map_err(|e| e.to_string())?;
        }
        w.finalize().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    };
    if let Err(e) = write() {
        doc.note = Some(format!("Couldn't save: {e}"));
        return;
    }
    let sample = crate::samples::name_for(&path);
    let table_id = p::osc(doc.osc, p::TABLE);
    let ch = &ed.p.channels[ed.ch];
    if ch.sample.as_deref() != Some(sample.as_str()) || ch.params[table_id] != IMPORTED as f32 {
        ed.h.checkpoint(ed.p);
        ed.p.channels[ed.ch].sample = Some(sample.clone());
        ed.put(table_id, IMPORTED as f32);
    }
    crate::samples::reload(&sample);
    // The editor's own picture of the import, refreshed now.
    let table = Table::from_frames(&doc.frames, false);
    ui.ctx().data_mut(|d| d.insert_temp(Id::new(("chrys-import", sample.as_str())), Some(Arc::new(table))));
    doc.dirty = false;
    let file = path.file_name().map_or_else(String::new, |f| f.to_string_lossy().into_owned());
    doc.note = Some(format!("saved · {file}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn harmonic_amp(f: &[f32], h: usize) -> f32 {
        let (re, im) = spectrum(f);
        2.0 * re[h].hypot(im[h]) / FRAME as f32
    }

    #[test]
    fn spectral_morph_moves_harmonics_and_crossfade_keeps_the_ends() {
        let a = shape(|t| (TAU * t).sin());
        let b = shape(|t| (TAU * 2.0 * t).sin());
        let mut frames = vec![a.clone(), vec![0.0; FRAME], b.clone()];
        morph(&mut frames, true);
        assert!((harmonic_amp(&frames[1], 1) - 0.5).abs() < 1e-3);
        assert!((harmonic_amp(&frames[1], 2) - 0.5).abs() < 1e-3);
        let mut frames = vec![a.clone(), vec![0.0; FRAME], vec![0.0; FRAME], b.clone()];
        morph(&mut frames, false);
        assert_eq!(frames[0], a);
        assert_eq!(frames[3], b);
        assert!((frames[1][FRAME / 4] - (2.0 / 3.0 + 1.0 / 3.0 * b[FRAME / 4])).abs() < 1e-5);
    }

    #[test]
    fn resize_keeps_the_ends_and_smooth_softens_the_top() {
        let a = shape(|t| (TAU * t).sin());
        let b = shape(|t| if t < 0.5 { 1.0 } else { -1.0 });
        let r = resize(&[a.clone(), b.clone()], 8);
        assert_eq!(r.len(), 8);
        assert_eq!(r[0], a);
        assert_eq!(r[7], b);
        let mut s = b.clone();
        smooth(&mut s);
        assert!(harmonic_amp(&s, 201) < 0.8 * harmonic_amp(&b, 201));
        assert!((harmonic_amp(&s, 1) - harmonic_amp(&b, 1)).abs() < 0.01);
        assert_eq!(file_name("My / table: v2"), "My _ table_ v2.wav");
        assert_eq!(file_name("..."), "table.wav");
    }
}
