//! Developer switches, driven by environment variables. They let tests and
//! screenshots run without anyone touching the mouse — or the speakers.
//!
//! | variable | effect |
//! |---|---|
//! | `LYRA_SCREENSHOT=out.png` | after warm-up, save a screenshot of the window and quit |
//! | `LYRA_SCREENSHOT_FRAMES=n` | warm-up frames before the shot (default 45) |
//! | `LYRA_PLAY=1` | start playing on launch |
//! | `LYRA_SILENT=1` | engine runs normally, but the sound card gets zeros (see `audio.rs`) |
//! | `LYRA_OPEN=piano,settings` | open these windows on launch |
//! | `LYRA_BUFFER=n` | audio ring size in frames (see `audio.rs`) |
//! | `LYRA_SCRIPT="…"` | synthetic input, one step per frame (see below) |
//!
//! Script steps, separated by `;`: `wait N` (frames), `move X Y` (points),
//! `down` / `up` (left button), `rdown` / `rup` (right button), `click`,
//! `wheel DY`, `key NAME` (an egui key name, e.g. `Space`, `ArrowUp`),
//! `mods alt|shift|ctrl|none` (held modifiers, e.g. `mods alt+shift`),
//! `log` (print the step counter). Example — drag the first volume knob up:
//! `LYRA_SCRIPT="wait 30; move 71 134; down; move 71 120; move 71 100; up"`

use egui::{Context, ViewportCommand};
use std::path::PathBuf;

pub struct DevTools {
    script: Vec<String>,
    script_wait: u32,
    pointer: egui::Pos2,
    mods: egui::Modifiers,
    screenshot: Option<PathBuf>,
    frames_left: u32,
    requested: bool,
    pub autoplay: bool,
    pub open: Vec<String>,
}

fn flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty() && v != "0")
}

impl DevTools {
    pub fn from_env() -> Self {
        let script = std::env::var("LYRA_SCRIPT")
            .map(|s| s.split(';').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).rev().collect())
            .unwrap_or_default();
        Self {
            script,
            script_wait: 0,
            pointer: egui::Pos2::ZERO,
            mods: egui::Modifiers::NONE,
            screenshot: std::env::var_os("LYRA_SCREENSHOT").map(PathBuf::from),
            frames_left: std::env::var("LYRA_SCREENSHOT_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(45),
            requested: false,
            autoplay: flag("LYRA_PLAY"),
            open: std::env::var("LYRA_OPEN")
                .map(|v| v.split(',').map(|s| s.trim().to_lowercase()).collect())
                .unwrap_or_default(),
        }
    }

    /// Feed one scripted step into this frame's raw input.
    pub fn inject(&mut self, ctx: &Context, raw: &mut egui::RawInput) {
        if self.script.is_empty() {
            return;
        }
        ctx.request_repaint();
        raw.modifiers = self.mods;
        if self.script_wait > 0 {
            self.script_wait -= 1;
            return;
        }
        let Some(step) = self.script.pop() else { return };
        let parts: Vec<&str> = step.split_whitespace().collect();
        let num = |i: usize| parts.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let mods = self.mods;
        let button = move |pressed: bool, button: egui::PointerButton, pos: egui::Pos2| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: mods,
        };
        let pos = self.pointer;
        match parts.first().copied().unwrap_or("") {
            "wait" => self.script_wait = num(1) as u32,
            "move" => {
                self.pointer = egui::pos2(num(1), num(2));
                raw.events.push(egui::Event::PointerMoved(self.pointer));
            }
            "down" => raw.events.push(button(true, egui::PointerButton::Primary, pos)),
            "up" => raw.events.push(button(false, egui::PointerButton::Primary, pos)),
            "rdown" => raw.events.push(button(true, egui::PointerButton::Secondary, pos)),
            "rup" => raw.events.push(button(false, egui::PointerButton::Secondary, pos)),
            "click" => {
                raw.events.push(button(true, egui::PointerButton::Primary, pos));
                raw.events.push(button(false, egui::PointerButton::Primary, pos));
            }
            "wheel" => raw.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, num(1)),
                modifiers: egui::Modifiers::NONE,
            }),
            "key" => {
                if let Some(key) = parts.get(1).and_then(|n| egui::Key::from_name(n)) {
                    for pressed in [true, false] {
                        raw.events.push(egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
            }
            "mods" => {
                let spec = parts.get(1).copied().unwrap_or("none");
                self.mods = egui::Modifiers {
                    alt: spec.contains("alt"),
                    shift: spec.contains("shift"),
                    ctrl: spec.contains("ctrl"),
                    command: spec.contains("ctrl"),
                    mac_cmd: false,
                };
                raw.modifiers = self.mods;
            }
            "log" => eprintln!("lyra-script: {} steps left", self.script.len()),
            other => eprintln!("lyra-script: unknown step {other:?}"),
        }
    }

    pub fn wants(&self, window: &str) -> bool {
        self.open.iter().any(|w| w == window)
    }

    /// Call once per frame.
    pub fn frame(&mut self, ctx: &Context) {
        let Some(path) = &self.screenshot else { return };
        ctx.request_repaint(); // keep frames coming so the countdown advances
        if !self.requested {
            self.frames_left = self.frames_left.saturating_sub(1);
            if self.frames_left == 0 {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
                self.requested = true;
            }
            return;
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            match write_png(path, &image) {
                Ok(()) => eprintln!("lyra: screenshot saved to {}", path.display()),
                Err(e) => eprintln!("lyra: screenshot failed: {e}"),
            }
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

/// Minimal PNG encoder (uncompressed deflate). Big files, zero dependencies.
fn write_png(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    let [w, h] = img.size;
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for row in img.pixels.chunks(w) {
        raw.push(0); // filter: none
        for c in row {
            raw.extend_from_slice(&c.to_srgba_unmultiplied());
        }
    }

    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [(b"IHDR", &ihdr[..]), (b"IDAT", &z[..]), (b"IEND", &[][..])] {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = png.len();
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let crc = crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }
    std::fs::write(path, png)
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &x in data {
        crc ^= x as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
