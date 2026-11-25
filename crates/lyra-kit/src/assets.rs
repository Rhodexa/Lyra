//! A plugin's artwork, from its own folder: built into the program, and while
//! Lyra runs from the source tree also watched on disk. Export over the file
//! (from Krita, say) and the editor shows it within a second, no restart.
//! Transparent margins are trimmed, so draw freely.
//!
//! ```ignore
//! const LOGO: Artwork = Artwork {
//!     name: "chrysalis-emblem",
//!     built_in: include_bytes!("../assets/emblem.png"),
//!     file: concat!(env!("CARGO_MANIFEST_DIR"), "/assets/emblem.png"),
//!     max_h: 220,
//! };
//! let (tex, size) = LOGO.texture(ui.ctx());
//! ```

use egui::{Color32, ColorImage, Context, TextureHandle};

/// One piece of artwork (a PNG).
pub struct Artwork {
    /// Unique among every plugin's artwork (the texture's name).
    pub name: &'static str,
    /// The copy built into the program (`include_bytes!`).
    pub built_in: &'static [u8],
    /// Where the file lives in the source tree (`concat!(env!("CARGO_MANIFEST_DIR"), …)`).
    pub file: &'static str,
    /// Scaled down to at most this many pixels tall.
    pub max_h: usize,
}

#[derive(Clone)]
struct Cache {
    handle: TextureHandle,
    size: [usize; 2],
    modified: Option<std::time::SystemTime>,
    checked: std::time::Instant,
}

impl Artwork {
    /// The texture and its pixel size (for the aspect ratio): the file if it is
    /// there (re-read when it changes), else the built-in copy.
    pub fn texture(&self, ctx: &Context) -> (egui::TextureId, [usize; 2]) {
        let id = egui::Id::new(("artwork", self.name));
        // Copy the cache out first: egui's context lock is not re-entrant.
        let cached = ctx.data(|d| d.get_temp::<Cache>(id));
        let now = std::time::Instant::now();
        if let Some(c) = &cached {
            if now.duration_since(c.checked) < std::time::Duration::from_secs(1) {
                return (c.handle.id(), c.size);
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        let modified = std::fs::metadata(self.file).and_then(|m| m.modified()).ok();
        if let Some(c) = cached.clone().filter(|c| c.modified == modified) {
            ctx.data_mut(|d| d.insert_temp(id, Cache { checked: now, ..c.clone() }));
            return (c.handle.id(), c.size);
        }
        // New or changed: decode the file if it's there (and readable), else the built-in copy.
        let from_file = modified.and_then(|_| std::fs::read(self.file).ok()).and_then(|b| decode(&b, self.max_h));
        let image = from_file
            .or_else(|| decode(self.built_in, self.max_h))
            .unwrap_or_else(|| ColorImage::new([1, 1], Color32::TRANSPARENT));
        let size = image.size;
        let handle = match cached {
            Some(mut c) => {
                c.handle.set(image, egui::TextureOptions::LINEAR);
                c.handle
            }
            None => ctx.load_texture(self.name, image, egui::TextureOptions::LINEAR),
        };
        ctx.data_mut(|d| d.insert_temp(id, Cache { handle: handle.clone(), size, modified, checked: now }));
        (handle.id(), size)
    }
}

/// PNG → RGBA, trimmed to its visible pixels and scaled down to `max_h`.
fn decode(bytes: &[u8], max_h: usize) -> Option<ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let px: Vec<[u8; 4]> = match info.color_type {
        png::ColorType::Rgba => buf.as_chunks::<4>().0.to_vec(),
        png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().map(|&[r, g, b]| [r, g, b, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.as_chunks::<2>().0.iter().map(|&[g, a]| [g, g, g, a]).collect(),
        png::ColorType::Grayscale => buf.iter().map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    let px = &px[..w * h];
    // Trim transparent margins.
    let visible = |x: usize, y: usize| px[y * w + x][3] > 8;
    let rows: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| visible(x, y))).collect();
    let cols: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| visible(x, y))).collect();
    let (&y0, &y1, &x0, &x1) = (rows.first()?, rows.last()?, cols.first()?, cols.last()?);
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    // Area-average down to max_h (premultiplied, so edges don't go dark).
    let (tw, th) = if ch > max_h { ((cw * max_h / ch).max(1), max_h) } else { (cw, ch) };
    let mut out = Vec::with_capacity(tw * th * 4);
    for ty in 0..th {
        let (sy0, sy1) = (y0 + ty * ch / th, y0 + ((ty + 1) * ch / th).max(ty * ch / th + 1));
        for tx in 0..tw {
            let (sx0, sx1) = (x0 + tx * cw / tw, x0 + ((tx + 1) * cw / tw).max(tx * cw / tw + 1));
            let mut acc = [0u32; 4];
            for y in sy0..sy1 {
                for x in sx0..sx1 {
                    let [r, g, b, a] = px[y * w + x];
                    let a32 = a as u32;
                    acc[0] += r as u32 * a32;
                    acc[1] += g as u32 * a32;
                    acc[2] += b as u32 * a32;
                    acc[3] += a32;
                }
            }
            let n = ((sx1 - sx0) * (sy1 - sy0)) as u32;
            let a = acc[3].max(1);
            out.extend_from_slice(&[(acc[0] / a) as u8, (acc[1] / a) as u8, (acc[2] / a) as u8, (acc[3] / n) as u8]);
        }
    }
    Some(ColorImage::from_rgba_unmultiplied([tw, th], &out))
}
