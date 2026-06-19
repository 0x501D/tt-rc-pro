use std::collections::HashMap;

use ab_glyph::{Font as AbFont, FontArc, PxScale, ScaleFont};
use image::imageops;
use image::ImageEncoder;
use image::{Rgb, RgbImage, RgbaImage};
use imageproc::drawing::{
    draw_filled_rect_mut, draw_hollow_rect_mut, draw_line_segment_mut, draw_text_mut,
};
use imageproc::rect::Rect;

use crate::config::{BarId, Config, DragTarget, ElementConfig, ElementId};
use crate::gif::GifAnimation;
use crate::sensor::SensorData;
use crate::{H, W};

/// Caches loaded fonts by path so they are not re-read from disk every frame.
pub struct FontCache {
    cache: HashMap<String, FontArc>,
    default_bold: FontArc,
    default_regular: FontArc,
}

impl FontCache {
    pub fn new(config: &Config) -> Self {
        let default_bold = load_font(&config.default_font_bold);
        let default_regular = load_font(&config.default_font_regular);
        FontCache {
            cache: HashMap::new(),
            default_bold,
            default_regular,
        }
    }

    /// Get the resolved font for an element. Loads and caches if new path.
    pub fn get_font(&mut self, config: &Config, elem: &ElementConfig) -> &FontArc {
        let path = config.resolve_font_path(elem).to_owned();
        if path == config.default_font_bold {
            return &self.default_bold;
        }
        if path == config.default_font_regular {
            return &self.default_regular;
        }
        if !self.cache.contains_key(&path) {
            let font = load_font(&path);
            self.cache.insert(path.clone(), font);
        }
        self.cache.get(&path).unwrap()
    }

    /// Reload default fonts (call when default paths change).
    pub fn reload_defaults(&mut self, config: &Config) {
        self.default_bold = load_font(&config.default_font_bold);
        self.default_regular = load_font(&config.default_font_regular);
    }
}

/// Opaque font type used by rendering (FontArc owns the font data).
pub type Font = FontArc;

/// Load a font from disk. Exits with error if not found.
pub fn load_font(path: &str) -> Font {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("Cannot load font {path}: {e}"));
    FontArc::try_from_vec(data).unwrap_or_else(|e| panic!("Invalid font {path}: {e}"))
}

/// Dynamic temperature color: green → yellow → red based on thresholds.
pub fn temp_color(temp: Option<f32>, warn: f32, crit: f32) -> Rgb<u8> {
    match temp {
        None => Rgb([0x88, 0x88, 0x88]),
        Some(t) if t >= crit => Rgb([0xff, 0x33, 0x33]),
        Some(t) if t >= warn => Rgb([0xff, 0xaa, 0x00]),
        _ => Rgb([0x44, 0xff, 0x88]),
    }
}

fn draw_bar(
    img: &mut RgbImage,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    pct: f32,
    fill_color: Rgb<u8>,
    bg_color: Rgb<u8>,
    border_color: Rgb<u8>,
) {
    // Background.
    draw_filled_rect_mut(img, Rect::at(x, y).of_size(w, h), bg_color);
    // Fill.
    let fw = (w as f32 * pct.min(100.0) / 100.0) as u32;
    if fw > 0 {
        draw_filled_rect_mut(img, Rect::at(x, y).of_size(fw, h), fill_color);
    }
    // Outline.
    draw_hollow_rect_mut(img, Rect::at(x, y).of_size(w, h), border_color);
}

/// Draw a line graph within the given rectangle.
fn draw_line_graph(
    img: &mut RgbImage,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    data: &[f32],
    max_val: f32,
    line_color: Rgb<u8>,
    bg_color: Rgb<u8>,
    border_color: Rgb<u8>,
) {
    // Background.
    draw_filled_rect_mut(img, Rect::at(x, y).of_size(w, h), bg_color);

    if data.len() >= 2 {
        // Auto-scale Y axis if max_val is 0.
        let effective_max = if max_val > 0.0 {
            max_val
        } else {
            data.iter()
                .cloned()
                .fold(f32::NEG_INFINITY, f32::max)
                .max(1.0)
        };

        let inner_w = w as f32;
        let inner_h = h as f32;
        let n = data.len();
        let step = inner_w / (n - 1).max(1) as f32;

        for i in 0..(n - 1) {
            let x0 = x as f32 + i as f32 * step;
            let x1 = x as f32 + (i + 1) as f32 * step;
            let y0 = y as f32 + inner_h - (data[i].min(effective_max) / effective_max * inner_h);
            let y1 =
                y as f32 + inner_h - (data[i + 1].min(effective_max) / effective_max * inner_h);
            draw_line_segment_mut(img, (x0, y0), (x1, y1), line_color);
        }
    }

    // Border.
    draw_hollow_rect_mut(img, Rect::at(x, y).of_size(w, h), border_color);
}

/// Resolve the effective color for a text element.
fn elem_color(
    elem: &ElementConfig,
    id: ElementId,
    data_temp: Option<f32>,
    warn: f32,
    crit: f32,
) -> Rgb<u8> {
    if elem.use_dynamic_color && id.supports_dynamic_color() {
        temp_color(data_temp, warn, crit)
    } else {
        Rgb(elem.color)
    }
}

/// GIF compositing.
///
/// Alpha-blend an RGBA overlay onto an RGB base image.
/// Only touches pixels within the overlay region.
fn blend_rgba_on_rgb(base: &mut RgbImage, overlay: &RgbaImage, ox: i32, oy: i32) {
    for (dx, dy, pixel) in overlay.enumerate_pixels() {
        let bx = ox + dx as i32;
        let by = oy + dy as i32;
        if bx < 0 || by < 0 || bx >= base.width() as i32 || by >= base.height() as i32 {
            continue;
        }
        let alpha = pixel[3] as f32 / 255.0;
        if alpha == 0.0 {
            continue; // Fully transparent.
        }
        let bp = base.get_pixel(bx as u32, by as u32);
        let blended = Rgb([
            (bp[0] as f32 * (1.0 - alpha) + pixel[0] as f32 * alpha) as u8,
            (bp[1] as f32 * (1.0 - alpha) + pixel[1] as f32 * alpha) as u8,
            (bp[2] as f32 * (1.0 - alpha) + pixel[2] as f32 * alpha) as u8,
        ]);
        base.put_pixel(bx as u32, by as u32, blended);
    }
}

/// Overlay the current GIF frame onto the image, scaled to config dimensions.
fn render_gif_overlay(img: &mut RgbImage, config: &Config, gif: &GifAnimation) {
    let frame = gif.current_frame();

    // Determine target size: use config values, or original GIF size if 0.
    let target_w = if config.gif.width > 0 {
        config.gif.width
    } else {
        gif.original_width
    };
    let target_h = if config.gif.height > 0 {
        config.gif.height
    } else {
        gif.original_height
    };

    // Scale the frame if needed.
    let scaled = if frame.width() != target_w || frame.height() != target_h {
        imageops::resize(frame, target_w, target_h, imageops::FilterType::Triangle)
    } else {
        frame.clone()
    };

    blend_rgba_on_rgb(img, &scaled, config.gif.x, config.gif.y);
}

/// Render a frame to an RgbImage (used for preview and JPEG encoding).
pub fn render_frame(
    data: &SensorData,
    config: &Config,
    fonts: &mut FontCache,
    gif: Option<&GifAnimation>,
) -> RgbImage {
    let mut img = RgbImage::from_pixel(W, H, Rgb(config.background_color));

    // Divider.
    let d = &config.divider;
    if d.visible {
        draw_line_segment_mut(
            &mut img,
            (d.x as f32, d.y_start as f32),
            (d.x as f32, d.y_end as f32),
            Rgb(d.color),
        );
    }

    // Left panel: CPU.
    if let Some(e) = config.elements.get(&ElementId::CpuTempLabel) {
        if e.visible {
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(&mut img, Rgb(e.color), e.x, e.y, scale, font, "CPU TEMP");
        }
    }

    if let Some(e) = config.elements.get(&ElementId::CpuTempValue) {
        if e.visible {
            let cpu_str = match data.cpu_temp {
                Some(t) => format!("{t:.1}\u{00b0}"),
                None => "N/A".into(),
            };
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            let color = elem_color(e, ElementId::CpuTempValue, data.cpu_temp, 70.0, 85.0);
            draw_text_mut(&mut img, color, e.x, e.y, scale, font, &cpu_str);
        }
    }

    if let Some(e) = config.elements.get(&ElementId::CpuLoad) {
        if e.visible {
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(
                &mut img,
                Rgb(e.color),
                e.x,
                e.y,
                scale,
                font,
                &format!("LOAD  {:.0}%", data.cpu_pct),
            );
        }
    }

    if let Some(b) = config.bars.get(&BarId::CpuLoad) {
        if b.visible {
            draw_bar(
                &mut img,
                b.x,
                b.y,
                b.width,
                b.height,
                data.cpu_pct,
                Rgb(b.fill_color),
                Rgb(b.bg_color),
                Rgb(b.border_color),
            );
        }
    }

    if let Some(e) = config.elements.get(&ElementId::Ram) {
        if e.visible {
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(
                &mut img,
                Rgb(e.color),
                e.x,
                e.y,
                scale,
                font,
                &format!("RAM   {:.1}/{:.0} GB", data.ram_used_gb, data.ram_total_gb),
            );
        }
    }

    if let Some(b) = config.bars.get(&BarId::Ram) {
        if b.visible {
            draw_bar(
                &mut img,
                b.x,
                b.y,
                b.width,
                b.height,
                data.ram_pct,
                Rgb(b.fill_color),
                Rgb(b.bg_color),
                Rgb(b.border_color),
            );
        }
    }

    // Right panel: GPU + NVMe + time.
    if let Some(e) = config.elements.get(&ElementId::GpuTempLabel) {
        if e.visible {
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(&mut img, Rgb(e.color), e.x, e.y, scale, font, "GPU TEMP");
        }
    }

    if let Some(e) = config.elements.get(&ElementId::GpuTempValue) {
        if e.visible {
            let gpu_str = match data.gpu_temp {
                Some(t) => format!("{t:.1}\u{00b0}"),
                None => "N/A".into(),
            };
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            let color = elem_color(e, ElementId::GpuTempValue, data.gpu_temp, 75.0, 90.0);
            draw_text_mut(&mut img, color, e.x, e.y, scale, font, &gpu_str);
        }
    }

    // GPU Load text.
    if let Some(e) = config.elements.get(&ElementId::GpuLoad) {
        if e.visible {
            let gpu_load_str = match data.gpu_load_pct {
                Some(pct) => format!("GPU LOAD  {:.0}%", pct),
                None => "GPU LOAD  N/A".into(),
            };
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(&mut img, Rgb(e.color), e.x, e.y, scale, font, &gpu_load_str);
        }
    }

    // GPU Load bar.
    if let Some(b) = config.bars.get(&BarId::GpuLoad) {
        if b.visible {
            let pct = data.gpu_load_pct.unwrap_or(0.0);
            draw_bar(
                &mut img,
                b.x,
                b.y,
                b.width,
                b.height,
                pct,
                Rgb(b.fill_color),
                Rgb(b.bg_color),
                Rgb(b.border_color),
            );
        }
    }

    // VRAM text.
    if let Some(e) = config.elements.get(&ElementId::GpuVram) {
        if e.visible {
            let vram_str = if data.vram_total_gb > 0.0 {
                format!(
                    "VRAM  {:.1}/{:.0} GB",
                    data.vram_used_gb, data.vram_total_gb
                )
            } else {
                "VRAM  N/A".into()
            };
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(&mut img, Rgb(e.color), e.x, e.y, scale, font, &vram_str);
        }
    }

    // VRAM bar.
    if let Some(b) = config.bars.get(&BarId::GpuVram) {
        if b.visible {
            draw_bar(
                &mut img,
                b.x,
                b.y,
                b.width,
                b.height,
                data.vram_pct,
                Rgb(b.fill_color),
                Rgb(b.bg_color),
                Rgb(b.border_color),
            );
        }
    }

    // FPS text.
    if let Some(e) = config.elements.get(&ElementId::Fps) {
        if e.visible {
            let fps_str = match data.fps {
                Some(f) => format!("FPS {:.0}", f),
                None => "FPS --".into(),
            };
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(&mut img, Rgb(e.color), e.x, e.y, scale, font, &fps_str);
        }
    }

    // Frametime text.
    if let Some(e) = config.elements.get(&ElementId::Frametime) {
        if e.visible {
            let ft_str = match data.frametime_ms {
                Some(ft) => format!("FT {:.1}ms", ft),
                None => "FT --".into(),
            };
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(&mut img, Rgb(e.color), e.x, e.y, scale, font, &ft_str);
        }
    }

    if let Some(e) = config.elements.get(&ElementId::NvmeLabel) {
        if e.visible {
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(&mut img, Rgb(e.color), e.x, e.y, scale, font, "NVME");
        }
    }

    if let Some(e) = config.elements.get(&ElementId::NvmeValue) {
        if e.visible {
            let nvme_str = match data.nvme_temp {
                Some(t) => format!("{t:.1}\u{00b0}"),
                None => "N/A".into(),
            };
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            let color = elem_color(e, ElementId::NvmeValue, data.nvme_temp, 55.0, 70.0);
            draw_text_mut(&mut img, color, e.x, e.y, scale, font, &nvme_str);
        }
    }

    // Time & Date.
    let now = local_time();

    if let Some(e) = config.elements.get(&ElementId::Time) {
        if e.visible {
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(&mut img, Rgb(e.color), e.x, e.y, scale, font, &now.time_str);
        }
    }

    if let Some(e) = config.elements.get(&ElementId::Date) {
        if e.visible {
            let font = fonts.get_font(config, e);
            let scale = PxScale {
                x: e.font_size,
                y: e.font_size,
            };
            draw_text_mut(&mut img, Rgb(e.color), e.x, e.y, scale, font, &now.date_str);
        }
    }

    // Frametime graph.
    let g = &config.frametime_graph;
    if g.visible && !data.frametime_history.is_empty() {
        draw_line_graph(
            &mut img,
            g.x,
            g.y,
            g.width,
            g.height,
            &data.frametime_history,
            g.max_ms,
            Rgb(g.line_color),
            Rgb(g.bg_color),
            Rgb(g.border_color),
        );
    }

    // GIF overlay (rendered last, on top of everything).
    if config.gif.visible {
        if let Some(gif) = gif {
            render_gif_overlay(&mut img, config, gif);
        }
    }

    img
}

/// Local time representation.
struct LocalTime {
    time_str: String,
    date_str: String,
}

fn local_time() -> LocalTime {
    let output = std::process::Command::new("date")
        .args(["+%H:%M:%S%n%m/%d"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            let mut lines = s.lines();
            let time_str = lines.next().unwrap_or("00:00:00").to_string();
            let date_str = lines.next().unwrap_or("01/01").to_string();
            LocalTime { time_str, date_str }
        }
        _ => LocalTime {
            time_str: "??".into(),
            date_str: "??".into(),
        },
    }
}

/// Encode an RgbImage as JPEG with the given quality.
pub fn encode_jpeg(img: &RgbImage, quality: u8) -> Vec<u8> {
    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    encoder
        .write_image(img.as_raw(), W, H, image::ExtendedColorType::Rgb8)
        .expect("JPEG encoding failed");
    buf
}

/// Convenience: render + encode in one step (for LCD thread).
pub fn make_frame(
    data: &SensorData,
    config: &Config,
    fonts: &mut FontCache,
    gif: Option<&GifAnimation>,
) -> Vec<u8> {
    let img = render_frame(data, config, fonts, gif);
    encode_jpeg(&img, config.jpeg_quality)
}

/// Bounding boxes for hit testing.
///
/// Compute bounding boxes for all visible elements, used for drag-and-drop hit testing.
/// Returns a list of (DragTarget, (x, y, width, height)).
pub fn compute_bounding_boxes(
    data: &SensorData,
    config: &Config,
    fonts: &mut FontCache,
) -> Vec<(DragTarget, (i32, i32, u32, u32))> {
    let mut boxes = Vec::new();

    // Helper: compute text bounding box.
    let text_bbox =
        |font: &FontArc, font_size: f32, text: &str, x: i32, y: i32| -> (i32, i32, u32, u32) {
            let scale = PxScale {
                x: font_size,
                y: font_size,
            };
            let scaled = font.as_scaled(scale);
            let width: f32 = text
                .chars()
                .map(|c| scaled.h_advance(scaled.glyph_id(c)))
                .sum();
            (x, y, width.ceil() as u32, font_size.ceil() as u32)
        };

    let now = local_time();

    // Pre-format all dynamic text strings to avoid lifetime issues.
    let cpu_temp_str = match data.cpu_temp {
        Some(t) => format!("{t:.1}\u{00b0}"),
        None => "N/A".into(),
    };
    let cpu_load_str = format!("LOAD  {:.0}%", data.cpu_pct);
    let ram_str = format!("RAM   {:.1}/{:.0} GB", data.ram_used_gb, data.ram_total_gb);
    let gpu_temp_str = match data.gpu_temp {
        Some(t) => format!("{t:.1}\u{00b0}"),
        None => "N/A".into(),
    };
    let gpu_load_str = match data.gpu_load_pct {
        Some(pct) => format!("LOAD  {:.0}%", pct),
        None => "LOAD  N/A".into(),
    };
    let vram_str = if data.vram_total_gb > 0.0 {
        format!(
            "VRAM  {:.1}/{:.0} GB",
            data.vram_used_gb, data.vram_total_gb
        )
    } else {
        "VRAM  N/A".into()
    };
    let fps_str = match data.fps {
        Some(f) => format!("FPS {:.0}", f),
        None => "FPS --".into(),
    };
    let ft_str = match data.frametime_ms {
        Some(ft) => format!("FT {:.1}ms", ft),
        None => "FT --".into(),
    };
    let nvme_temp_str = match data.nvme_temp {
        Some(t) => format!("{t:.1}\u{00b0}"),
        None => "N/A".into(),
    };

    // Text elements with borrowed strings.
    let text_items: Vec<(ElementId, &str)> = vec![
        (ElementId::CpuTempLabel, "CPU TEMP"),
        (ElementId::CpuTempValue, &cpu_temp_str),
        (ElementId::CpuLoad, &cpu_load_str),
        (ElementId::Ram, &ram_str),
        (ElementId::GpuTempLabel, "GPU TEMP"),
        (ElementId::GpuTempValue, &gpu_temp_str),
        (ElementId::GpuLoad, &gpu_load_str),
        (ElementId::GpuVram, &vram_str),
        (ElementId::Fps, &fps_str),
        (ElementId::Frametime, &ft_str),
        (ElementId::NvmeLabel, "NVME"),
        (ElementId::NvmeValue, &nvme_temp_str),
        (ElementId::Time, &now.time_str),
        (ElementId::Date, &now.date_str),
    ];

    for (id, text) in &text_items {
        if let Some(e) = config.elements.get(id) {
            if e.visible {
                let font = fonts.get_font(config, e);
                let bbox = text_bbox(font, e.font_size, text, e.x, e.y);
                boxes.push((DragTarget::Element(*id), bbox));
            }
        }
    }

    // Bar elements.
    for id in BarId::all() {
        if let Some(b) = config.bars.get(id) {
            if b.visible {
                boxes.push((DragTarget::Bar(*id), (b.x, b.y, b.width, b.height)));
            }
        }
    }

    // Frametime graph.
    if config.frametime_graph.visible {
        let g = &config.frametime_graph;
        boxes.push((DragTarget::FrametimeGraph, (g.x, g.y, g.width, g.height)));
    }

    // Divider.
    if config.divider.visible {
        let d = &config.divider;
        boxes.push((
            DragTarget::Divider,
            (d.x - 2, d.y_start, 4, (d.y_end - d.y_start) as u32),
        ));
    }

    // GIF.
    if config.gif.visible && config.gif.path.is_some() {
        let g = &config.gif;
        let w = if g.width > 0 { g.width } else { 32 }; // fallback if no GIF loaded yet
        let h = if g.height > 0 { g.height } else { 32 };
        boxes.push((DragTarget::Gif, (g.x, g.y, w, h)));
    }

    boxes
}
