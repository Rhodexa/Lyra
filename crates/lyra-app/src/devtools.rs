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

use egui::{Context, ViewportCommand};
use std::path::PathBuf;

pub struct DevTools {
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
        Self {
            screenshot: std::env::var_os("LYRA_SCREENSHOT").map(PathBuf::from),
            frames_left: std::env::var("LYRA_SCREENSHOT_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(45),
            requested: false,
            autoplay: flag("LYRA_PLAY"),
            open: std::env::var("LYRA_OPEN")
                .map(|v| v.split(',').map(|s| s.trim().to_lowercase()).collect())
                .unwrap_or_default(),
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
