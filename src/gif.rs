use std::fs::File;
use std::io::BufReader;
use std::time::Instant;

use anyhow::{Context, Result};
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, RgbaImage};

/// Decoded animated GIF with time-based frame selection.
pub struct GifAnimation {
    frames: Vec<RgbaImage>,
    delays_ms: Vec<u64>,
    total_duration_ms: u64,
    start: Instant,
    /// Original (unscaled) dimensions of the first frame.
    pub original_width: u32,
    pub original_height: u32,
}

impl GifAnimation {
    /// Load and decode all frames of an animated GIF.
    pub fn load(path: &str) -> Result<Self> {
        let file =
            File::open(path).with_context(|| format!("Cannot open GIF: {path}"))?;
        let reader = BufReader::new(file);
        let decoder = GifDecoder::new(reader)
            .with_context(|| format!("Cannot decode GIF: {path}"))?;

        let mut frames = Vec::new();
        let mut delays_ms = Vec::new();
        let mut original_width = 0u32;
        let mut original_height = 0u32;

        for frame_result in decoder.into_frames() {
            let frame = frame_result
                .with_context(|| format!("Error decoding GIF frame in: {path}"))?;

            let delay = frame.delay();
            // Delay::numer_denom_ms() returns (numerator, denominator) in ms
            let (numer, denom) = delay.numer_denom_ms();
            let delay_ms = if denom > 0 {
                (numer as u64) / (denom as u64)
            } else {
                100 // fallback: 100ms
            };
            delays_ms.push(delay_ms.max(10)); // minimum 10ms to avoid 0-delay frames

            let buffer = frame.buffer().clone();
            if frames.is_empty() {
                original_width = buffer.width();
                original_height = buffer.height();
            }
            frames.push(buffer);
        }

        if frames.is_empty() {
            anyhow::bail!("GIF has no frames: {path}");
        }

        let total_duration_ms = delays_ms.iter().sum();

        Ok(GifAnimation {
            frames,
            delays_ms,
            total_duration_ms,
            start: Instant::now(),
            original_width,
            original_height,
        })
    }

    /// Returns the index of the frame that should be displayed right now.
    /// The animation loops indefinitely.
    pub fn current_frame_index(&self) -> usize {
        if self.frames.len() <= 1 {
            return 0;
        }
        let elapsed_ms = self.start.elapsed().as_millis() as u64;
        let pos_in_loop = elapsed_ms % self.total_duration_ms;

        let mut accumulated = 0u64;
        for (i, &delay) in self.delays_ms.iter().enumerate() {
            accumulated += delay;
            if pos_in_loop < accumulated {
                return i;
            }
        }
        self.frames.len() - 1
    }

    /// Get the current frame's pixel data (based on wall-clock time).
    pub fn current_frame(&self) -> &RgbaImage {
        &self.frames[self.current_frame_index()]
    }

    /// Number of frames in the animation.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
}
