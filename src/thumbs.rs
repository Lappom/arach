//! Pictures shrunk to the size they draw at, kept on disk beside the art.
//!
//! Rune decodes a JPEG or PNG at its full size whatever size it is asked for, then keeps
//! those pixels (and their mips) in memory as long as anything draws them. A Steam hero is
//! 3840×1240, a cover 600×900, a screenshot 4K: a library page showing every cover held
//! hundreds of megabytes. Here each picture is decoded once, off the UI thread, downscaled to
//! the width it shows at, and saved; Rune then only ever loads the small copy.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use image::imageops::FilterType;

/// A picture and the width, in pixels, it is wanted at.
pub type Key = (PathBuf, u32);

/// Decoders running at once: enough to fill a screen of covers quickly, few enough that a
/// cold library doesn't hold many full-size decodes at the same time.
const WORKERS: usize = 2;

/// Rounds a width up to a few steps, so a cover or hero resized with the window reuses one
/// copy instead of making a new one for every pixel of width.
pub fn bucket(px: u32) -> u32 {
    let step = match px {
        0..=128 => 16,
        129..=512 => 64,
        _ => 256,
    };
    px.max(1).div_ceil(step) * step
}

pub fn dir() -> PathBuf {
    crate::store::cache_dir().join("thumbs")
}

pub struct Thumbs {
    jobs: Sender<Key>,
    done: Receiver<(Key, PathBuf)>,
}

impl Thumbs {
    pub fn new(waker: rune::Waker) -> Thumbs {
        let (jobs, queue) = channel::<Key>();
        let (finished, done) = channel();
        let queue = Arc::new(Mutex::new(queue));
        for _ in 0..WORKERS {
            let (queue, finished, waker) = (queue.clone(), finished.clone(), waker.clone());
            let _ = std::thread::Builder::new().name("arach-thumbs".into()).spawn(move || loop {
                let Ok(key) = queue.lock().unwrap().recv() else { return };
                // Anything that can't be shrunk (already small, an SVG, an unknown format)
                // loads as it is, as it did before.
                let file = shrink(&key.0, key.1).unwrap_or_else(|| key.0.clone());
                if finished.send((key, file)).is_err() { return }
                waker();
            });
        }
        Thumbs { jobs, done }
    }

    pub fn request(&self, key: Key) {
        let _ = self.jobs.send(key);
    }

    /// Each picture resolved since the last call, with the file to load for it.
    pub fn finished(&self) -> impl Iterator<Item = (Key, PathBuf)> + '_ {
        self.done.try_iter()
    }
}

/// The small copy of `src` at `px` wide, made now if it isn't on disk yet; `None` when the
/// original should load instead.
fn shrink(src: &Path, px: u32) -> Option<PathBuf> {
    let meta = std::fs::metadata(src).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (src, meta.len(), mtime, px).hash(&mut h);
    let stem = format!("{:016x}-{px}", h.finish());
    let dir = dir();
    for ext in ["jpg", "png"] {
        let file = dir.join(format!("{stem}.{ext}"));
        if file.is_file() { return Some(file) }
    }
    // Only the header: a picture already about this size isn't worth a copy.
    let (w, h) = image::image_dimensions(src).ok()?;
    if w <= px + px / 8 { return None }
    let img = image::open(src).ok()?;
    let th = ((h as u64 * px as u64) / w as u64).max(1) as u32;
    let small = img.resize_exact(px, th, FilterType::Triangle);
    drop(img);
    std::fs::create_dir_all(&dir).ok()?;
    let alpha = small.color().has_alpha();
    let file = dir.join(format!("{stem}.{}", if alpha { "png" } else { "jpg" }));
    // Written aside then renamed, so a half-written file is never loaded.
    let tmp = file.with_extension("tmp");
    let saved = if alpha {
        small.save_with_format(&tmp, image::ImageFormat::Png).is_ok()
    } else {
        std::fs::File::create(&tmp).ok().is_some_and(|f| {
            let mut out = std::io::BufWriter::new(f);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88).encode_image(&small.to_rgb8()).is_ok()
        })
    };
    if !saved || std::fs::rename(&tmp, &file).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    Some(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_share_a_few_steps() {
        assert_eq!(bucket(24), 32);
        assert_eq!(bucket(300), 320);
        assert_eq!(bucket(304), 320);
        assert_eq!(bucket(1900), 2048);
        assert_eq!(bucket(1920), 2048);
        assert_eq!(bucket(0), 16);
    }
}
