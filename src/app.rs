use std::sync::{Arc, RwLock};

use egui::color_picker::Alpha;
use egui::*;

use crate::config::{BarId, Config, DragTarget, ElementId, FontWeight};
use crate::gif::GifAnimation;
use crate::lcd_thread::LcdState;
use crate::render::{self, FontCache};
use crate::{H, W};

// ── App state ──────────────────────────────────────────────────────────────────

pub struct TtRcApp {
    config: Arc<RwLock<Config>>,
    lcd_state: Arc<RwLock<LcdState>>,
    font_cache: FontCache,
    gif: Option<GifAnimation>,
    preview_texture: Option<TextureHandle>,
    selected: Option<DragTarget>,
    drag_state: Option<DragState>,
    preview_scale: f32,
    /// Track last GIF path to detect changes for reload.
    last_gif_path: Option<String>,
}

struct DragState {
    target: DragTarget,
    start_mouse_img: (f32, f32),
    start_pos: (i32, i32),
}

impl TtRcApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        config: Arc<RwLock<Config>>,
        lcd_state: Arc<RwLock<LcdState>>,
    ) -> Self {
        let cfg = config.read().unwrap().clone();
        let font_cache = FontCache::new(&cfg);

        // Try to load GIF if path is set
        let gif = cfg
            .gif
            .path
            .as_deref()
            .and_then(|p| GifAnimation::load(p).ok());
        let last_gif_path = cfg.gif.path.clone();

        Self {
            config,
            lcd_state,
            font_cache,
            gif,
            preview_texture: None,
            selected: None,
            drag_state: None,
            preview_scale: 2.0,
            last_gif_path,
        }
    }

    /// Get current config as a cloned value (for rendering without holding the lock).
    fn config_clone(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    /// Check if GIF path changed and reload if needed.
    fn maybe_reload_gif(&mut self) {
        let current_path = self.config.read().unwrap().gif.path.clone();
        if current_path != self.last_gif_path {
            self.gif = current_path
                .as_deref()
                .and_then(|p| GifAnimation::load(p).ok());
            self.last_gif_path = current_path;
        }
    }

    /// Render the preview image and return the texture handle.
    fn ensure_preview_texture(&mut self, ctx: &Context) -> &TextureHandle {
        let sensor_data = self.lcd_state.read().unwrap().last_sensor_data.clone();
        let config = self.config_clone();

        let img = render::render_frame(&sensor_data, &config, &mut self.font_cache, self.gif.as_ref());

        // Convert RgbImage to egui ColorImage
        let raw = img.as_raw();
        let pixels: Vec<u8> = raw
            .chunks(3)
            .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
            .collect();
        let color_image =
            ColorImage::from_rgba_unmultiplied([W as usize, H as usize], &pixels);

        if let Some(ref mut texture) = self.preview_texture {
            texture.set(color_image, TextureOptions::LINEAR);
        } else {
            self.preview_texture = Some(
                ctx.load_texture("preview", color_image, TextureOptions::LINEAR),
            );
        }
        self.preview_texture.as_ref().unwrap()
    }

    /// Draw a highlight rectangle around the selected element on the preview.
    fn draw_selection_highlight(
        &self,
        painter: &Painter,
        preview_rect: Rect,
        bboxes: &[(DragTarget, (i32, i32, u32, u32))],
    ) {
        let Some(ref sel) = self.selected else { return };
        let Some((_, (bx, by, bw, bh))) = bboxes.iter().find(|(t, _)| t == sel) else {
            return;
        };

        let scale = self.preview_scale;
        let x = preview_rect.min.x + *bx as f32 * scale;
        let y = preview_rect.min.y + *by as f32 * scale;
        let w = *bw as f32 * scale;
        let h = *bh as f32 * scale;

        painter.rect_stroke(
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h)),
            0.0,
            Stroke::new(2.0, Color32::from_rgb(255, 200, 0)),
            StrokeKind::Outside,
        );
    }

    /// Hit test: find which element is at the given image coordinates.
    fn hit_test(
        &self,
        img_x: i32,
        img_y: i32,
        bboxes: &[(DragTarget, (i32, i32, u32, u32))],
    ) -> Option<DragTarget> {
        for (target, (bx, by, bw, bh)) in bboxes.iter().rev() {
            if img_x >= *bx
                && img_x < *bx + *bw as i32
                && img_y >= *by
                && img_y < *by + *bh as i32
            {
                return Some(*target);
            }
        }
        None
    }

    /// Compute bounding boxes, consuming config and sensor data.
    fn compute_bboxes(&mut self) -> Vec<(DragTarget, (i32, i32, u32, u32))> {
        let sensor_data = self.lcd_state.read().unwrap().last_sensor_data.clone();
        let config = self.config_clone();
        let mut bboxes = render::compute_bounding_boxes(&sensor_data, &config, &mut self.font_cache);

        // Override GIF bbox with actual dimensions from loaded GIF
        if config.gif.visible && config.gif.path.is_some() {
            // Remove the placeholder GIF bbox
            bboxes.retain(|(t, _)| !matches!(t, DragTarget::Gif));
            // Add accurate bbox
            let (w, h) = if let Some(ref gif) = self.gif {
                let gw = if config.gif.width > 0 { config.gif.width } else { gif.original_width };
                let gh = if config.gif.height > 0 { config.gif.height } else { gif.original_height };
                (gw, gh)
            } else {
                (32, 32) // fallback placeholder
            };
            bboxes.push((DragTarget::Gif, (config.gif.x, config.gif.y, w, h)));
        }

        bboxes
    }
}

impl eframe::App for TtRcApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Check if GIF path changed
        self.maybe_reload_gif();

        // ── Status bar (bottom) ────────────────────────────────────────────────
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Thermaltake RC Pro LCD Config");
                ui.separator();
                let state = self.lcd_state.read().unwrap();
                if state.device_connected {
                    ui.colored_label(Color32::GREEN, "● LCD Connected");
                } else {
                    ui.colored_label(Color32::RED, "○ LCD Disconnected");
                }
                ui.separator();
                let d = &state.last_sensor_data;
                ui.label(format!(
                    "CPU {:.1}°  GPU {:.1}°  Load {:.0}%",
                    d.cpu_temp.unwrap_or(0.0),
                    d.gpu_temp.unwrap_or(0.0),
                    d.cpu_pct,
                ));
                ui.separator();
                let interval = self.config.read().unwrap().update_interval_secs;
                ui.label(format!("Update: {interval}s"));
            });
        });

        // ── Right panel: controls ──────────────────────────────────────────────
        SidePanel::right("controls_panel")
            .min_width(300.0)
            .default_width(320.0)
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    self.draw_controls(ui);
                });
            });

        // ── Top panel: zoom controls ───────────────────────────────────────────
        TopBottomPanel::top("zoom_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Zoom:");
                if ui.button("➖").on_hover_text("Zoom out").clicked() {
                    self.preview_scale = (self.preview_scale - 0.5).max(0.5);
                }
                if ui.button("➕").on_hover_text("Zoom in").clicked() {
                    self.preview_scale = (self.preview_scale + 0.5).min(4.0);
                }
                if ui.button("1:1").on_hover_text("Original size (1×)").clicked() {
                    self.preview_scale = 1.0;
                }
                if ui.button("Fit").on_hover_text("Fit to available space").clicked() {
                    // Calculate scale based on central panel size minus some padding
                    let central_size = ctx.screen_rect().size();
                    let side_panel_width = 320.0; // approx side panel
                    let top_bottom_height = 60.0; // approx top+bottom bars
                    let avail_w = central_size.x - side_panel_width - 20.0;
                    let avail_h = central_size.y - top_bottom_height - 40.0;
                    let scale_x = avail_w / W as f32;
                    let scale_y = avail_h / H as f32;
                    self.preview_scale = scale_x.min(scale_y).max(0.5).min(4.0);
                    // Round to nearest 0.25 for cleaner display
                    self.preview_scale = (self.preview_scale * 4.0).round() / 4.0;
                }
                ui.label(format!("{:.2}×", self.preview_scale));
                ui.separator();
                ui.label(format!("{W}×{H}"));
            });
        });

        // ── Central panel: live preview ────────────────────────────────────────
        CentralPanel::default().show(ctx, |ui| {
            // Compute bboxes before rendering (needs &mut self.font_cache)
            let bboxes = self.compute_bboxes();

            // Render preview and get texture
            let texture = self.ensure_preview_texture(ctx);
            let tex_size = texture.size_vec2();
            let scale = self.preview_scale;
            let display_size = tex_size * scale;

            // Center the preview
            let available = ui.available_size();
            let offset_x = ((available.x - display_size.x) / 2.0).max(0.0);
            let offset_y = ((available.y - display_size.y) / 2.0).max(0.0);

            ui.allocate_ui_with_layout(
                available,
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.allocate_space(Vec2::new(offset_x, 0.0));
                    ui.vertical_centered(|ui| {
                        ui.allocate_space(Vec2::new(0.0, offset_y));

                        let (rect, response) =
                            ui.allocate_exact_size(display_size, Sense::click_and_drag());

                        // Draw the preview image
                        let tex = self.preview_texture.as_ref().unwrap();
                        ui.put(
                            rect,
                            Image::new(tex).fit_to_exact_size(display_size),
                        );

                        // Draw selection highlight
                        let painter = ui.painter_at(rect);
                        self.draw_selection_highlight(&painter, rect, &bboxes);

                        // ── Drag & drop handling ────────────────────────────────
                        if response.drag_started() {
                            if let Some(pos) = response.interact_pointer_pos() {
                                let img_x = ((pos.x - rect.min.x) / scale) as i32;
                                let img_y = ((pos.y - rect.min.y) / scale) as i32;
                                if let Some(target) = self.hit_test(img_x, img_y, &bboxes) {
                                    self.selected = Some(target);
                                    let config = self.config_clone();
                                    let start_pos = config.get_pos(&target);
                                    self.drag_state = Some(DragState {
                                        target,
                                        start_mouse_img: (img_x as f32, img_y as f32),
                                        start_pos,
                                    });
                                }
                            }
                        }

                        if response.dragged() {
                            if let Some(ref drag) = self.drag_state {
                                if let Some(pos) = response.interact_pointer_pos() {
                                    let img_x = (pos.x - rect.min.x) / scale;
                                    let img_y = (pos.y - rect.min.y) / scale;
                                    let dx = img_x as i32 - drag.start_mouse_img.0 as i32;
                                    let dy = img_y as i32 - drag.start_mouse_img.1 as i32;
                                    let new_x =
                                        (drag.start_pos.0 + dx).clamp(0, W as i32 - 1);
                                    let new_y =
                                        (drag.start_pos.1 + dy).clamp(0, H as i32 - 1);
                                    let mut cfg = self.config.write().unwrap();
                                    cfg.set_pos(&drag.target, new_x, new_y);
                                }
                            }
                        }

                        if response.drag_stopped() {
                            self.drag_state = None;
                        }

                        // Click without drag = select only
                        if response.clicked() && self.drag_state.is_none() {
                            if let Some(pos) = response.interact_pointer_pos() {
                                let img_x = ((pos.x - rect.min.x) / scale) as i32;
                                let img_y = ((pos.y - rect.min.y) / scale) as i32;
                                self.selected = self.hit_test(img_x, img_y, &bboxes);
                            }
                        }
                    });
                },
            );

        });

        // Repaint continuously for live preview
        ctx.request_repaint();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Auto-save config on exit
        let cfg = self.config.read().unwrap();
        if let Err(e) = cfg.save() {
            eprintln!("Failed to save config on exit: {e}");
        }
    }
}

// ── Controls panel drawing ─────────────────────────────────────────────────────

impl TtRcApp {
    fn draw_controls(&mut self, ui: &mut Ui) {
        ui.heading("Elements");
        ui.separator();

        // ── Element list with visibility toggles ───────────────────────────────
        for id in ElementId::all() {
            let mut cfg = self.config.write().unwrap();
            let elem = cfg.element_mut(*id);
            let name = id.display_name();
            let mut visible = elem.visible;

            ui.horizontal(|ui| {
                ui.checkbox(&mut visible, name);
                if ui.small_button("☰").on_hover_text("Select").clicked() {
                    self.selected = Some(DragTarget::Element(*id));
                }
            });

            if elem.visible != visible {
                elem.visible = visible;
            }
        }

        // ── Bar list ───────────────────────────────────────────────────────────
        for id in BarId::all() {
            let mut cfg = self.config.write().unwrap();
            let bar = cfg.bar_mut(*id);
            let name = id.display_name();
            let mut visible = bar.visible;

            ui.horizontal(|ui| {
                ui.checkbox(&mut visible, name);
                if ui.small_button("☰").on_hover_text("Select").clicked() {
                    self.selected = Some(DragTarget::Bar(*id));
                }
            });

            if bar.visible != visible {
                bar.visible = visible;
            }
        }

        // ── Divider ────────────────────────────────────────────────────────────
        {
            let mut cfg = self.config.write().unwrap();
            let mut visible = cfg.divider.visible;
            ui.horizontal(|ui| {
                ui.checkbox(&mut visible, "Divider");
                if ui.small_button("☰").on_hover_text("Select").clicked() {
                    self.selected = Some(DragTarget::Divider);
                }
            });
            if cfg.divider.visible != visible {
                cfg.divider.visible = visible;
            }
        }

        // ── GIF ────────────────────────────────────────────────────────────────
        {
            let mut cfg = self.config.write().unwrap();
            let mut visible = cfg.gif.visible;
            ui.horizontal(|ui| {
                ui.checkbox(&mut visible, "GIF Animation");
                if ui.small_button("☰").on_hover_text("Select").clicked() {
                    self.selected = Some(DragTarget::Gif);
                }
            });
            if cfg.gif.visible != visible {
                cfg.gif.visible = visible;
            }
        }

        ui.separator();

        // ── Selected element detail ────────────────────────────────────────────
        self.draw_selected_detail(ui);

        ui.separator();

        // ── Global settings ────────────────────────────────────────────────────
        self.draw_global_settings(ui);

        ui.separator();

        // ── Action buttons ─────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            if ui.button("💾 Save Config").clicked() {
                let cfg = self.config.read().unwrap();
                if let Err(e) = cfg.save() {
                    eprintln!("Save failed: {e}");
                } else {
                    println!("Config saved to {}", Config::config_path().display());
                }
            }
            if ui.button("📂 Load Config").clicked() {
                match Config::load() {
                    Ok(loaded) => {
                        *self.config.write().unwrap() = loaded;
                        let cfg = self.config.read().unwrap().clone();
                        self.font_cache.reload_defaults(&cfg);
                        println!("Config loaded");
                    }
                    Err(e) => eprintln!("Load failed: {e}"),
                }
            }
            if ui.button("🔄 Reset Defaults").clicked() {
                *self.config.write().unwrap() = Config::default();
                let cfg = self.config.read().unwrap().clone();
                self.font_cache.reload_defaults(&cfg);
                self.selected = None;
            }
        });
    }

    fn draw_selected_detail(&mut self, ui: &mut Ui) {
        let Some(ref sel) = self.selected else {
            ui.label("Select an element to edit");
            return;
        };

        match sel {
            DragTarget::Element(id) => self.draw_element_detail(ui, *id),
            DragTarget::Bar(id) => self.draw_bar_detail(ui, *id),
            DragTarget::Divider => self.draw_divider_detail(ui),
            DragTarget::Gif => self.draw_gif_detail(ui),
        }
    }

    fn draw_element_detail(&mut self, ui: &mut Ui, id: ElementId) {
        ui.heading(format!("{} Properties", id.display_name()));

        let mut cfg = self.config.write().unwrap();
        let elem = cfg.element_mut(id);

        // Position
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(
                DragValue::new(&mut elem.x)
                    .range(0..=W as i32 - 1)
                    .speed(1),
            );
            ui.label("Y:");
            ui.add(
                DragValue::new(&mut elem.y)
                    .range(0..=H as i32 - 1)
                    .speed(1),
            );
        });

        // Font size
        ui.horizontal(|ui| {
            ui.label("Font Size:");
            ui.add(
                DragValue::new(&mut elem.font_size)
                    .range(6.0..=72.0)
                    .speed(0.5),
            );
        });

        // Font weight
        ui.horizontal(|ui| {
            ui.label("Font Weight:");
            let mut is_bold = elem.font_weight == FontWeight::Bold;
            ui.radio_value(&mut is_bold, false, "Regular");
            ui.radio_value(&mut is_bold, true, "Bold");
            elem.font_weight = if is_bold {
                FontWeight::Bold
            } else {
                FontWeight::Regular
            };
        });

        // Custom font path
        ui.horizontal(|ui| {
            ui.label("Font:");
            let mut path_str = elem.font_path.clone().unwrap_or_default();
            let response = ui.add(
                TextEdit::singleline(&mut path_str)
                    .hint_text("(default)")
                    .desired_width(180.0),
            );
            if response.changed() {
                if path_str.is_empty() {
                    elem.font_path = None;
                } else {
                    elem.font_path = Some(path_str);
                }
            }
        });

        // Color
        ui.horizontal(|ui| {
            ui.label("Color:");
            let mut color32 = color8_to_color32(elem.color);
            if color_picker::color_picker_color32(ui, &mut color32, Alpha::Opaque) {
                elem.color = color32_to_color8(color32);
            }
        });

        // Dynamic color (for temperature elements)
        if id.supports_dynamic_color() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut elem.use_dynamic_color, "Dynamic color (temp-based)");
            });
            if elem.use_dynamic_color {
                ui.label("  Color changes: green → yellow → red");
            }
        }
    }

    fn draw_bar_detail(&mut self, ui: &mut Ui, id: BarId) {
        ui.heading(format!("{} Properties", id.display_name()));

        let mut cfg = self.config.write().unwrap();
        let bar = cfg.bar_mut(id);

        // Position
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(
                DragValue::new(&mut bar.x)
                    .range(0..=W as i32 - 1)
                    .speed(1),
            );
            ui.label("Y:");
            ui.add(
                DragValue::new(&mut bar.y)
                    .range(0..=H as i32 - 1)
                    .speed(1),
            );
        });

        // Dimensions
        ui.horizontal(|ui| {
            ui.label("Width:");
            ui.add(DragValue::new(&mut bar.width).range(4..=W).speed(1));
            ui.label("Height:");
            ui.add(
                DragValue::new(&mut bar.height)
                    .range(2..=40)
                    .speed(1),
            );
        });

        // Colors
        ui.horizontal(|ui| {
            ui.label("Fill:");
            let mut c = color8_to_color32(bar.fill_color);
            if color_picker::color_picker_color32(ui, &mut c, Alpha::Opaque) {
                bar.fill_color = color32_to_color8(c);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Background:");
            let mut c = color8_to_color32(bar.bg_color);
            if color_picker::color_picker_color32(ui, &mut c, Alpha::Opaque) {
                bar.bg_color = color32_to_color8(c);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Border:");
            let mut c = color8_to_color32(bar.border_color);
            if color_picker::color_picker_color32(ui, &mut c, Alpha::Opaque) {
                bar.border_color = color32_to_color8(c);
            }
        });
    }

    fn draw_divider_detail(&mut self, ui: &mut Ui) {
        ui.heading("Divider Properties");

        let mut cfg = self.config.write().unwrap();

        // Position
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(
                DragValue::new(&mut cfg.divider.x)
                    .range(0..=W as i32 - 1)
                    .speed(1),
            );
        });

        // Y range
        ui.horizontal(|ui| {
            ui.label("Y Start:");
            ui.add(
                DragValue::new(&mut cfg.divider.y_start)
                    .range(0..=H as i32 - 1)
                    .speed(1),
            );
            ui.label("Y End:");
            ui.add(
                DragValue::new(&mut cfg.divider.y_end)
                    .range(0..=H as i32 - 1)
                    .speed(1),
            );
        });

        // Color
        ui.horizontal(|ui| {
            ui.label("Color:");
            let mut c = color8_to_color32(cfg.divider.color);
            if color_picker::color_picker_color32(ui, &mut c, Alpha::Opaque) {
                cfg.divider.color = color32_to_color8(c);
            }
        });
    }

    fn draw_gif_detail(&mut self, ui: &mut Ui) {
        ui.heading("GIF Animation Properties");

        let mut cfg = self.config.write().unwrap();

        // GIF file path
        ui.horizontal(|ui| {
            ui.label("File:");
            let mut path_str = cfg.gif.path.clone().unwrap_or_default();
            let response = ui.add(
                TextEdit::singleline(&mut path_str)
                    .hint_text("/path/to/animation.gif")
                    .desired_width(200.0),
            );
            if response.changed() {
                if path_str.is_empty() {
                    cfg.gif.path = None;
                } else {
                    cfg.gif.path = Some(path_str);
                }
            }
        });

        // GIF status
        if let Some(ref gif) = self.gif {
            ui.label(format!(
                "Loaded: {} frames, {}×{}",
                gif.frame_count(),
                gif.original_width,
                gif.original_height,
            ));
        } else if cfg.gif.path.is_some() {
            ui.colored_label(Color32::from_rgb(255, 100, 100), "⚠ Failed to load GIF");
        } else {
            ui.label("No GIF selected");
        }

        // Position
        ui.horizontal(|ui| {
            ui.label("X:");
            ui.add(
                DragValue::new(&mut cfg.gif.x)
                    .range(0..=W as i32 - 1)
                    .speed(1),
            );
            ui.label("Y:");
            ui.add(
                DragValue::new(&mut cfg.gif.y)
                    .range(0..=H as i32 - 1)
                    .speed(1),
            );
        });

        // Size (0 = original)
        ui.horizontal(|ui| {
            ui.label("Width:");
            ui.add(
                DragValue::new(&mut cfg.gif.width)
                    .range(0..=W)
                    .speed(1),
            );
            ui.label("Height:");
            ui.add(
                DragValue::new(&mut cfg.gif.height)
                    .range(0..=H)
                    .speed(1),
            );
        });
        ui.horizontal(|ui| {
            if ui.button("Original Size").clicked() {
                cfg.gif.width = 0;
                cfg.gif.height = 0;
            }
        });
        ui.label("(0 = use original GIF dimensions)");
    }

    fn draw_global_settings(&mut self, ui: &mut Ui) {
        ui.heading("Global Settings");

        let mut cfg = self.config.write().unwrap();

        // Background color
        ui.horizontal(|ui| {
            ui.label("Background:");
            let mut c = color8_to_color32(cfg.background_color);
            if color_picker::color_picker_color32(ui, &mut c, Alpha::Opaque) {
                cfg.background_color = color32_to_color8(c);
            }
        });

        // Default fonts
        ui.horizontal(|ui| {
            ui.label("Bold Font:");
            let response = ui.add(
                TextEdit::singleline(&mut cfg.default_font_bold)
                    .desired_width(200.0),
            );
            if response.changed() {
                self.font_cache.reload_defaults(&*self.config.read().unwrap());
            }
        });

        ui.horizontal(|ui| {
            ui.label("Regular Font:");
            let response = ui.add(
                TextEdit::singleline(&mut cfg.default_font_regular)
                    .desired_width(200.0),
            );
            if response.changed() {
                self.font_cache.reload_defaults(&*self.config.read().unwrap());
            }
        });

        // Update interval
        ui.horizontal(|ui| {
            ui.label("Update interval (s):");
            ui.add(
                DragValue::new(&mut cfg.update_interval_secs)
                    .range(1..=30)
                    .speed(1),
            );
        });

        // JPEG quality
        ui.horizontal(|ui| {
            ui.label("JPEG quality:");
            ui.add(
                DragValue::new(&mut cfg.jpeg_quality)
                    .range(10..=100)
                    .speed(1),
            );
        });
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn color8_to_color32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn color32_to_color8(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}
