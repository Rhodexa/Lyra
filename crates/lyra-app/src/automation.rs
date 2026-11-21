//! Automation, the app's side: what a target *is* (its knob law, its value,
//! its name), and how automation clips become the engine's lanes.
//!
//! An automation (`lyra_model::Automation`) is a shared curve, like a
//! pattern: its clips place it on the arrangement, each with its own window
//! (start, length, offset; longer than the curve = it repeats). One curve can
//! move several controls (its `links`), each over its own range.
//!
//! In the engine every automated control is one lane: the curve along the
//! whole song, as knob travel. Between clips a control *holds* its last
//! value; before its first clip it rests where its knob is. Starting the
//! song anywhere puts every control where the song has it ("chase").

use lyra_engine::{curve_at, Addr, AutoLane};
use lyra_model::{Automation, Clip, Project, Target};
use lyra_plugin::{Curve, ParamInfo};

// ── What the knobs show ──
//
// The engine moves automated controls itself and leaves the project's own
// values alone (they are where each control rests). So the editors can show
// automation moving, the app works out every automated value at the playhead
// once per frame while the song plays (the curve depends only on the song
// position) and the editors read their knobs through `shown`. It is the
// host's job, like a CLAP host telling a plugin's GUI about automation: no
// plugin is involved.

thread_local! {
    static SHOWN: std::cell::RefCell<Vec<(Addr, f32)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// This frame's automated values (empty when the song isn't playing). UI thread.
pub fn set_shown(values: Vec<(Addr, f32)>) {
    SHOWN.with(|s| *s.borrow_mut() = values);
}

/// Where automation has a control right now, if it's automated and playing.
pub fn shown(addr: Addr) -> Option<f32> {
    SHOWN.with(|s| s.borrow().iter().find(|(a, _)| *a == addr).map(|(_, v)| *v))
}

/// …for an instrument parameter (channel index, parameter id).
pub fn shown_param(channel: usize, id: usize) -> Option<f32> {
    shown(Addr::Param { channel, id: id as u32 })
}

/// Mixer faders (see `mixer.rs`): the same law as the plugin rack's volume.
static MIXER_VOLUME: ParamInfo = ParamInfo::new(0, "Volume", 0.0, 2.0, 1.0, "gain", Curve::Fader);

/// A target as a control: its knob law, its current value, and a name for
/// people ("Bass · Cutoff"). None when it no longer exists.
pub fn control(p: &Project, t: Target) -> Option<(ParamInfo, f32, String)> {
    let channel = |id| p.channels.iter().find(|c| c.id == id);
    Some(match t {
        Target::Param { channel: id, param } => {
            let ch = channel(id)?;
            let info = *lyra_catalog::params(&ch.instrument).get(param as usize)?;
            (info, *ch.params.get(param as usize)?, format!("{} · {}", ch.name, info.name))
        }
        Target::Volume(id) => {
            let ch = channel(id)?;
            (crate::rack::VOLUME, ch.volume, format!("{} · Volume", ch.name))
        }
        Target::Pan(id) => {
            let ch = channel(id)?;
            (crate::rack::PAN, ch.pan, format!("{} · Pan", ch.name))
        }
        Target::MixerVolume(t) => {
            let m = p.mixer.get(t)?;
            (MIXER_VOLUME, m.volume, format!("{} · Volume", m.name))
        }
        Target::MixerPan(t) => {
            let m = p.mixer.get(t)?;
            (crate::rack::PAN, m.pan, format!("{} · Pan", m.name))
        }
        Target::Effect { effect, param } => {
            let (t, s) = p.effect_slot(effect)?;
            let slot = &p.mixer[t].effects[s];
            let info = *lyra_catalog::effects::params(&slot.effect).get(param as usize)?;
            let fx = lyra_catalog::effects::info(&slot.effect).map_or("Effect", |e| e.name);
            (info, *slot.params.get(param as usize)?, format!("{} · {fx} · {}", p.mixer[t].name, info.name))
        }
        Target::MasterVolume => (crate::transport::MASTER, p.master_volume, "Master".to_string()),
    })
}

/// Where the engine finds a target (channel and slot indexes).
fn addr(p: &Project, t: Target) -> Option<Addr> {
    let index = |id| p.channels.iter().position(|c| c.id == id);
    Some(match t {
        Target::Param { channel, param } => Addr::Param { channel: index(channel)?, id: param },
        Target::Volume(c) => Addr::Volume(index(c)?),
        Target::Pan(c) => Addr::Pan(index(c)?),
        Target::MixerVolume(t) => Addr::MixerVolume(t),
        Target::MixerPan(t) => Addr::MixerPan(t),
        Target::Effect { effect, param } => {
            let (track, slot) = p.effect_slot(effect)?;
            Addr::Effect { track, slot, id: param }
        }
        Target::MasterVolume => Addr::Master,
    })
}

/// An automation's curve as (tick, value, tension) points.
pub fn points(a: &Automation) -> Vec<(f64, f32, f32)> {
    a.points.iter().map(|p| (p.tick as f64, p.value, p.tension)).collect()
}

/// The curve's value at `u` ticks into the automation, repeating every
/// `length` (and holding its first value before its first point).
pub fn value_at(a: &Automation, pts: &[(f64, f32, f32)], u: f64) -> f32 {
    let local = u.rem_euclid(a.length.max(1) as f64);
    curve_at(pts, local).unwrap_or_else(|| pts.first().map_or(0.0, |p| p.1))
}

/// What a clip plays of its automation: points relative to the clip's start,
/// from its window (offset, length) onto the curve repeated forever. Cut
/// segments at the window's ends are sampled, so bent lines stay bent.
pub fn window(a: &Automation, offset: u32, len: u32) -> Vec<(f64, f32, f32)> {
    const PIECES: usize = 8;
    let pts = points(a);
    let l = a.length.max(1) as f64;
    let (w0, w1) = (offset as f64, (offset + len) as f64);
    let value = |u: f64| value_at(a, &pts, u);
    // Points strictly inside the window, every repeat.
    let mut inner = Vec::new();
    for k in (w0 / l).floor() as i64..=(w1 / l).ceil() as i64 {
        for p in &a.points {
            let at = k as f64 * l + p.tick as f64;
            if at > w0 && at < w1 {
                inner.push((at, p.value, p.tension));
            }
        }
    }
    let sample = |from: f64, to: f64, out: &mut Vec<(f64, f32, f32)>| {
        for i in 0..PIECES {
            let u = from + (to - from) * i as f64 / PIECES as f64;
            out.push((u, value(u + 1e-9), 0.0));
        }
    };
    let mut out = Vec::new();
    // The cut segment before the first inner point (or the whole window).
    sample(w0, inner.first().map_or(w1, |p| p.0), &mut out);
    let last_inner = inner.len();
    for (i, p) in inner.iter().enumerate() {
        if i + 1 == last_inner {
            // Its line runs past the window's end: sample it up to the end.
            out.push((p.0, p.1, 0.0));
            sample(p.0, w1, &mut out);
            out.remove(out.len() - PIECES); // the sample at p.0 repeats p
        } else {
            out.push(*p);
        }
    }
    out.push((w1, value(w1 - 1e-6), 0.0));
    out.iter().map(|&(t, v, k)| (t - w0, v, k)).collect()
}

/// Every automation clip on the arrangement as engine lanes, one per
/// automated control.
pub fn compile(p: &Project) -> lyra_engine::Automation {
    // (target, clip, its automation, the link) in order of the clips' starts.
    let mut uses: Vec<(Target, &Clip, &Automation, lyra_model::Link)> = Vec::new();
    for clip in &p.clips {
        if p.tracks.get(clip.track).is_none_or(|t| t.mute) {
            continue;
        }
        let Some(a) = clip.automation().and_then(|id| p.automation(id)) else { continue };
        for link in &a.links {
            uses.push((link.target, clip, a, *link));
        }
    }
    uses.sort_by_key(|u| u.1.start);
    let mut targets: Vec<Target> = uses.iter().map(|u| u.0).collect();
    targets.dedup_by(|a, b| a == b);
    let mut seen = Vec::new();
    let mut lanes = Vec::new();
    for t in targets {
        if seen.contains(&t) {
            continue;
        }
        seen.push(t);
        let (Some((info, value, _)), Some(addr)) = (control(p, t), addr(p, t)) else { continue };
        let mine: Vec<_> = uses.iter().filter(|u| u.0 == t).collect();
        let mut points: Vec<(f64, f32, f32)> = Vec::new();
        for (k, (_, clip, a, link)) in mine.iter().enumerate() {
            // A clip starting before this one ends cuts it short.
            let end = mine.get(k + 1).map_or(clip.end(), |next| next.1.start.max(clip.start).min(clip.end()));
            let len = end - clip.start;
            if len == 0 {
                continue;
            }
            for (i, (at, v, tension)) in window(a, clip.offset, len).into_iter().enumerate() {
                let tick = clip.start as f64 + at;
                if i == 0 {
                    // Hold the last value up to here, then jump.
                    if let Some(&(_, last, _)) = points.last() {
                        points.push((tick, last, 0.0));
                    }
                }
                points.push((tick, link.travel(v), tension));
            }
        }
        lanes.push(AutoLane { addr, info, points, rest: info.to_normalized(value) });
    }
    lyra_engine::Automation { lanes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyra_model::{Target, TICKS_PER_BAR};

    fn ramp_project() -> (Project, Target) {
        let mut p = Project::default();
        let ch = p.add_channel("Bass", "lyra.synth", lyra_catalog::default_params("lyra.synth"));
        let t = Target::Volume(ch);
        let i = p.add_automation("Fade", t, 0.0, TICKS_PER_BAR);
        p.automations[i].points[1].value = 1.0; // 0 → 1 over a bar
        (p, t)
    }

    #[test]
    fn a_window_is_the_curve_repeated_and_cut() {
        let (p, _) = ramp_project();
        let a = &p.automations[0];
        let bar = TICKS_PER_BAR as f64;
        let at = |w: &[(f64, f32, f32)], t: f64| curve_at(w, t).unwrap();
        // Half a bar in, for a bar: the second half of the ramp, then the first half again.
        let w = window(a, TICKS_PER_BAR / 2, TICKS_PER_BAR);
        assert!((at(&w, 0.0) - 0.5).abs() < 1e-3);
        assert!((at(&w, bar * 0.25) - 0.75).abs() < 1e-3);
        assert!((at(&w, bar * 0.5 + 1.0) - 0.0).abs() < 0.01, "starts over");
        assert!((at(&w, bar) - 0.5).abs() < 0.01);
    }

    #[test]
    fn clips_become_lanes_that_hold_between_them() {
        let (mut p, t) = ramp_project();
        let id = p.automations[0].id;
        p.add_clip(Clip::of_automation(id, 0, 0, TICKS_PER_BAR / 2)); // 0 → 0.5
        p.add_clip(Clip::of_automation(id, 0, 2 * TICKS_PER_BAR, TICKS_PER_BAR)); // 0 → 1 again
        let set = compile(&p);
        assert_eq!(set.lanes.len(), 1);
        let lane = &set.lanes[0];
        assert!(matches!(lane.addr, Addr::Volume(0)));
        let travel = |t: u32| lane.travel_at(t as f64);
        assert!((travel(TICKS_PER_BAR) - 0.5).abs() < 0.01, "holds after the first clip");
        assert!((travel(2 * TICKS_PER_BAR) - 0.0).abs() < 0.01, "jumps at the next");
        assert!((travel(4 * TICKS_PER_BAR) - 1.0).abs() < 0.01, "holds at the end");
        // A link's range: the same curve moving the pan the other way.
        p.automations[0].links.push(lyra_model::Link { target: Target::Pan(p.channels[0].id), from: 1.0, to: 0.0 });
        let pan = compile(&p).lanes.into_iter().find(|l| matches!(l.addr, Addr::Pan(0))).unwrap();
        assert!((pan.travel_at(TICKS_PER_BAR as f64) - 0.5).abs() < 0.01);
        assert!((pan.travel_at(0.0) - 1.0).abs() < 0.01, "inverted");
        let _ = t;
    }
}
