//! Shared bits for the example songs: a tiny note language and patches from
//! presets. The songs only build projects; Lyra renders them (`--render`).
#![allow(dead_code)]

use lyra_model::{EffectSlot, Project, TICKS_PER_BAR, TICKS_PER_STEP};
use std::path::Path;

pub const STEP: u32 = TICKS_PER_STEP;
pub const BAR: u32 = TICKS_PER_BAR;

pub const D_MAJOR: &[(char, i32)] = &[('F', 1), ('C', 1)];
pub const F_MINOR: &[(char, i32)] = &[('B', -1), ('E', -1), ('A', -1), ('D', -1)];

#[derive(Clone, Copy, Debug)]
pub struct Tok {
    pub step: u32,
    pub len: u32,
    pub key: i32,
}

/// ABC-ish: one token = one step. Upper case = octave 4, lower case =
/// octave 5, ' up / , down, ^ sharp, _ flat, = natural, - hold, z or . rest.
/// `sig` is the key signature (letter, accidental).
pub fn parse(s: &str, sig: &[(char, i32)]) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    let mut step = 0;
    let mut acc: Option<i32> = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '|' | '\n' => {}
            '^' => acc = Some(1),
            '_' => acc = Some(-1),
            '=' => acc = Some(0),
            '-' => {
                if let Some(t) = out.last_mut() {
                    t.len += 1;
                }
                step += 1;
            }
            'z' | '.' => step += 1,
            'A'..='G' | 'a'..='g' => {
                let up = c.to_ascii_uppercase();
                let base = match up {
                    'C' => 0,
                    'D' => 2,
                    'E' => 4,
                    'F' => 5,
                    'G' => 7,
                    'A' => 9,
                    _ => 11,
                };
                let sig = sig.iter().find(|(l, _)| *l == up).map_or(0, |(_, a)| *a);
                let mut key = 60 + base + acc.take().unwrap_or(sig) + if c.is_lowercase() { 12 } else { 0 };
                while let Some(&m) = chars.peek() {
                    match m {
                        '\'' => key += 12,
                        ',' => key -= 12,
                        _ => break,
                    }
                    chars.next();
                }
                out.push(Tok { step, len: 1, key });
                step += 1;
            }
            _ => panic!("bad token {c:?}"),
        }
    }
    out
}

/// Fluttersine with its Init defaults plus `set`.
pub fn fm(set: &[(usize, f32)]) -> Vec<f32> {
    tweak(lyra_catalog::default_params("lyra.fluttersine"), set)
}

/// A Fluttersine preset by name, plus `set` on top.
pub fn fm_preset(name: &str, set: &[(usize, f32)]) -> Vec<f32> {
    let p = lyra_catalog::presets_for("lyra.fluttersine")
        .into_iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no preset {name}"));
    tweak(p.params(), set)
}

/// A built-in instrument's defaults (by id) plus `set`.
pub fn plugin(id: &str, set: &[(usize, f32)]) -> Vec<f32> {
    tweak(lyra_catalog::default_params(id), set)
}

pub fn tweak(mut v: Vec<f32>, set: &[(usize, f32)]) -> Vec<f32> {
    for &(i, x) in set {
        v[i] = x;
    }
    v
}

pub fn fx(id: &str, set: &[(usize, f32)]) -> EffectSlot {
    EffectSlot { effect: id.into(), params: tweak(lyra_catalog::effects::default_params(id), set), bypass: false }
}

/// Save the song as `<dir>/<title>.lyra` and say how to hear it. Rendering is
/// Lyra's job: `lyra --render <file>` (or open it and press play).
pub fn finish(p: &Project, dir: &Path, title: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(format!("{title}.{}", lyra_model::file::EXTENSION));
    std::fs::write(&path, lyra_model::file::to_json(p)).unwrap();
    let notes: usize = p.patterns.iter().map(|pt| pt.notes.values().map(Vec::len).sum::<usize>()).sum();
    println!(
        "{} channels · {} patterns · {} clips · {notes} notes · {} bars → {}",
        p.channels.len(),
        p.patterns.len(),
        p.clips.len(),
        p.song_length() / BAR,
        path.display()
    );
    println!("render it: cargo run --release -p lyra-app -- --render \"{}\"", path.display());
}
