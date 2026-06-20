use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Element identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum ElementId {
    CpuTempLabel,
    CpuTempValue,
    CpuLoad,
    Ram,
    GpuTempLabel,
    GpuTempValue,
    GpuLoad,
    GpuVram,
    Fps,
    Frametime,
    NvmeLabel,
    NvmeValue,
    Time,
    Date,
}

impl ElementId {
    /// All element IDs in display order.
    pub fn all() -> &'static [ElementId] {
        &[
            ElementId::CpuTempLabel,
            ElementId::CpuTempValue,
            ElementId::CpuLoad,
            ElementId::Ram,
            ElementId::GpuTempLabel,
            ElementId::GpuTempValue,
            ElementId::GpuLoad,
            ElementId::GpuVram,
            ElementId::Fps,
            ElementId::Frametime,
            ElementId::NvmeLabel,
            ElementId::NvmeValue,
            ElementId::Time,
            ElementId::Date,
        ]
    }

    /// Human-readable name for the GUI.
    pub fn display_name(&self) -> &'static str {
        match self {
            ElementId::CpuTempLabel => "CPU Temp Label",
            ElementId::CpuTempValue => "CPU Temp Value",
            ElementId::CpuLoad => "CPU Load",
            ElementId::Ram => "RAM Usage",
            ElementId::GpuTempLabel => "GPU Temp Label",
            ElementId::GpuTempValue => "GPU Temp Value",
            ElementId::GpuLoad => "GPU Load",
            ElementId::GpuVram => "VRAM Usage",
            ElementId::Fps => "FPS",
            ElementId::Frametime => "Frametime",
            ElementId::NvmeLabel => "NVMe Label",
            ElementId::NvmeValue => "NVMe Value",
            ElementId::Time => "Time",
            ElementId::Date => "Date",
        }
    }

    /// Whether this element supports dynamic color (temperature-based).
    pub fn supports_dynamic_color(&self) -> bool {
        matches!(
            self,
            ElementId::CpuTempValue | ElementId::GpuTempValue | ElementId::NvmeValue
        )
    }
}

/// Bar identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum BarId {
    CpuLoad,
    Ram,
    GpuLoad,
    GpuVram,
}

impl BarId {
    pub fn all() -> &'static [BarId] {
        &[BarId::CpuLoad, BarId::Ram, BarId::GpuLoad, BarId::GpuVram]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            BarId::CpuLoad => "CPU Load Bar",
            BarId::Ram => "RAM Bar",
            BarId::GpuLoad => "GPU Load Bar",
            BarId::GpuVram => "VRAM Bar",
        }
    }
}

/// Font weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum FontWeight {
    Bold,
    Regular,
}

impl Default for FontWeight {
    fn default() -> Self {
        FontWeight::Regular
    }
}

/// Drag target (unified identifier for hit testing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragTarget {
    Element(ElementId),
    Bar(BarId),
    Divider,
    Gif,
    FrametimeGraph,
}

/// Element configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ElementConfig {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default)]
    pub font_path: Option<String>,
    #[serde(default)]
    pub font_weight: FontWeight,
    pub color: [u8; 3],
    /// When true, temperature elements use dynamic color (green/yellow/red)
    /// instead of the fixed `color` field.
    #[serde(default)]
    pub use_dynamic_color: bool,
}

fn default_font_size() -> f32 {
    16.0
}

/// Bar configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BarConfig {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub fill_color: [u8; 3],
    pub bg_color: [u8; 3],
    pub border_color: [u8; 3],
}

/// Frametime graph configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrametimeGraphConfig {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub line_color: [u8; 3],
    pub bg_color: [u8; 3],
    pub border_color: [u8; 3],
    /// Y-axis max in ms (0 = auto-scale with headroom).
    #[serde(default = "default_graph_max_ms")]
    pub max_ms: f32,
}

fn default_graph_max_ms() -> f32 {
    50.0
}

/// Divider configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DividerConfig {
    pub visible: bool,
    pub x: i32,
    pub y_start: i32,
    pub y_end: i32,
    pub color: [u8; 3],
}

/// GIF configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GifConfig {
    pub visible: bool,
    /// Path to the GIF file. None = no GIF displayed.
    #[serde(default)]
    pub path: Option<String>,
    pub x: i32,
    pub y: i32,
    /// Display width. 0 = use original GIF width.
    #[serde(default)]
    pub width: u32,
    /// Display height. 0 = use original GIF height.
    #[serde(default)]
    pub height: u32,
}

fn default_gif() -> GifConfig {
    GifConfig {
        visible: false,
        path: None,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    }
}

fn default_fps_file_path() -> String {
    "/tmp/tt-rc-pro-fps".into()
}

fn default_frametime_graph() -> FrametimeGraphConfig {
    FrametimeGraphConfig {
        visible: false,
        x: 248,
        y: 112,
        width: 222,
        height: 14,
        line_color: [0x44, 0x88, 0xff],
        bg_color: [0x1a, 0x1a, 0x1a],
        border_color: [0x33, 0x33, 0x33],
        max_ms: 0.0,
    }
}

/// Top-level configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_elements")]
    pub elements: HashMap<ElementId, ElementConfig>,
    #[serde(default = "default_bars")]
    pub bars: HashMap<BarId, BarConfig>,
    #[serde(default = "default_divider")]
    pub divider: DividerConfig,
    #[serde(default = "default_gif")]
    pub gif: GifConfig,
    #[serde(default = "default_bg_color")]
    pub background_color: [u8; 3],
    #[serde(default = "default_font_bold")]
    pub default_font_bold: String,
    #[serde(default = "default_font_regular")]
    pub default_font_regular: String,
    #[serde(default = "default_interval")]
    pub update_interval_secs: u64,
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,
    /// Path to the external file providing FPS/frametime data.
    #[serde(default = "default_fps_file_path")]
    pub fps_file_path: String,
    /// Frametime line graph settings.
    #[serde(default = "default_frametime_graph")]
    pub frametime_graph: FrametimeGraphConfig,
}

fn default_font_bold() -> String {
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf".into()
}
fn default_font_regular() -> String {
    "/usr/share/fonts/TTF/DejaVuSans.ttf".into()
}
fn default_interval() -> u64 {
    2
}
fn default_jpeg_quality() -> u8 {
    92
}
fn default_bg_color() -> [u8; 3] {
    [8, 8, 8]
}

fn default_elements() -> HashMap<ElementId, ElementConfig> {
    let mut m = HashMap::new();
    // Left panel: CPU.
    m.insert(
        ElementId::CpuTempLabel,
        ElementConfig {
            visible: true,
            x: 8,
            y: 4,
            font_size: 13.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0x55, 0x55, 0x55],
            use_dynamic_color: false,
        },
    );
    m.insert(
        ElementId::CpuTempValue,
        ElementConfig {
            visible: true,
            x: 8,
            y: 20,
            font_size: 30.0,
            font_path: None,
            font_weight: FontWeight::Bold,
            color: [0x44, 0xff, 0x88],
            use_dynamic_color: true,
        },
    );
    m.insert(
        ElementId::CpuLoad,
        ElementConfig {
            visible: true,
            x: 8,
            y: 58,
            font_size: 16.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0xaa, 0xaa, 0xaa],
            use_dynamic_color: false,
        },
    );
    m.insert(
        ElementId::Ram,
        ElementConfig {
            visible: true,
            x: 8,
            y: 94,
            font_size: 16.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0xaa, 0xaa, 0xaa],
            use_dynamic_color: false,
        },
    );
    // Right panel: GPU.
    m.insert(
        ElementId::GpuTempLabel,
        ElementConfig {
            visible: true,
            x: 248,
            y: 4,
            font_size: 13.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0x55, 0x55, 0x55],
            use_dynamic_color: false,
        },
    );
    m.insert(
        ElementId::GpuTempValue,
        ElementConfig {
            visible: true,
            x: 248,
            y: 20,
            font_size: 30.0,
            font_path: None,
            font_weight: FontWeight::Bold,
            color: [0x44, 0xff, 0x88],
            use_dynamic_color: true,
        },
    );
    m.insert(
        ElementId::GpuLoad,
        ElementConfig {
            visible: false,
            x: 248,
            y: 58,
            font_size: 16.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0xaa, 0xaa, 0xaa],
            use_dynamic_color: false,
        },
    );
    m.insert(
        ElementId::GpuVram,
        ElementConfig {
            visible: false,
            x: 248,
            y: 78,
            font_size: 16.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0xaa, 0xaa, 0xaa],
            use_dynamic_color: false,
        },
    );
    m.insert(
        ElementId::Fps,
        ElementConfig {
            visible: false,
            x: 248,
            y: 96,
            font_size: 13.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0x88, 0xcc, 0x88],
            use_dynamic_color: false,
        },
    );
    m.insert(
        ElementId::Frametime,
        ElementConfig {
            visible: false,
            x: 340,
            y: 96,
            font_size: 13.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0x88, 0x88, 0xcc],
            use_dynamic_color: false,
        },
    );
    m.insert(
        ElementId::NvmeLabel,
        ElementConfig {
            visible: true,
            x: 248,
            y: 62,
            font_size: 13.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0x55, 0x55, 0x55],
            use_dynamic_color: false,
        },
    );
    m.insert(
        ElementId::NvmeValue,
        ElementConfig {
            visible: true,
            x: 248,
            y: 78,
            font_size: 16.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0x44, 0xff, 0x88],
            use_dynamic_color: true,
        },
    );
    m.insert(
        ElementId::Time,
        ElementConfig {
            visible: true,
            x: 248,
            y: 100,
            font_size: 16.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0x44, 0x44, 0x44],
            use_dynamic_color: false,
        },
    );
    m.insert(
        ElementId::Date,
        ElementConfig {
            visible: true,
            x: 352,
            y: 104,
            font_size: 13.0,
            font_path: None,
            font_weight: FontWeight::Regular,
            color: [0x33, 0x33, 0x33],
            use_dynamic_color: false,
        },
    );
    m
}

fn default_bars() -> HashMap<BarId, BarConfig> {
    let mut m = HashMap::new();
    m.insert(
        BarId::CpuLoad,
        BarConfig {
            visible: true,
            x: 8,
            y: 78,
            width: 222,
            height: 10,
            fill_color: [0x33, 0x77, 0xff],
            bg_color: [0x1a, 0x1a, 0x1a],
            border_color: [0x33, 0x33, 0x33],
        },
    );
    m.insert(
        BarId::Ram,
        BarConfig {
            visible: true,
            x: 8,
            y: 112,
            width: 222,
            height: 10,
            fill_color: [0x88, 0x44, 0xcc],
            bg_color: [0x1a, 0x1a, 0x1a],
            border_color: [0x33, 0x33, 0x33],
        },
    );
    m.insert(
        BarId::GpuLoad,
        BarConfig {
            visible: false,
            x: 248,
            y: 74,
            width: 222,
            height: 10,
            fill_color: [0xff, 0x88, 0x33],
            bg_color: [0x1a, 0x1a, 0x1a],
            border_color: [0x33, 0x33, 0x33],
        },
    );
    m.insert(
        BarId::GpuVram,
        BarConfig {
            visible: false,
            x: 248,
            y: 94,
            width: 222,
            height: 10,
            fill_color: [0xcc, 0x44, 0x88],
            bg_color: [0x1a, 0x1a, 0x1a],
            border_color: [0x33, 0x33, 0x33],
        },
    );
    m
}

fn default_divider() -> DividerConfig {
    DividerConfig {
        visible: true,
        x: 238,
        y_start: 6,
        y_end: 122,
        color: [0x2a, 0x2a, 0x2a],
    }
}

/// Which sensor groups are actually needed for rendering.
/// Derived from element/bar visibility so that hidden sensors are not read from disk.
#[derive(Debug, Clone)]
pub struct SensorNeeds {
    pub cpu_temp: bool,
    pub cpu_load: bool,
    pub ram: bool,
    pub gpu_temp: bool,
    pub gpu_load: bool,
    pub vram: bool,
    pub nvme_temp: bool,
    pub fps: bool,
}

impl SensorNeeds {
    /// All needs enabled (used when config is unavailable).
    #[allow(dead_code)]
    pub fn all() -> Self {
        SensorNeeds {
            cpu_temp: true,
            cpu_load: true,
            ram: true,
            gpu_temp: true,
            gpu_load: true,
            vram: true,
            nvme_temp: true,
            fps: true,
        }
    }

    /// No needs enabled.
    #[allow(dead_code)]
    pub fn none() -> Self {
        SensorNeeds {
            cpu_temp: false,
            cpu_load: false,
            ram: false,
            gpu_temp: false,
            gpu_load: false,
            vram: false,
            nvme_temp: false,
            fps: false,
        }
    }
}

/// Whether any element or bar with the given key is visible.
fn is_visible_elem(elements: &HashMap<ElementId, ElementConfig>, id: ElementId) -> bool {
    elements.get(&id).is_some_and(|e| e.visible)
}

fn is_visible_bar(bars: &HashMap<BarId, BarConfig>, id: BarId) -> bool {
    bars.get(&id).is_some_and(|b| b.visible)
}

impl Default for Config {
    fn default() -> Self {
        Config {
            elements: default_elements(),
            bars: default_bars(),
            divider: default_divider(),
            gif: default_gif(),
            background_color: default_bg_color(),
            default_font_bold: default_font_bold(),
            default_font_regular: default_font_regular(),
            update_interval_secs: default_interval(),
            jpeg_quality: default_jpeg_quality(),
            fps_file_path: default_fps_file_path(),
            frametime_graph: default_frametime_graph(),
        }
    }
}

impl Config {
    /// Path to the config file: ~/.config/tt-rc-pro/config.toml
    pub fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("tt-rc-pro").join("config.toml")
    }

    /// Load config from the default path.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("Cannot read {}", path.display()))?;
        let config: Self =
            toml::from_str(&text).with_context(|| format!("Cannot parse {}", path.display()))?;
        Ok(config)
    }

    /// Save config to the default path.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).context("Cannot serialize config")?;
        std::fs::write(&path, text).with_context(|| format!("Cannot write {}", path.display()))?;
        Ok(())
    }

    /// Compute which sensor groups are needed based on element/bar visibility.
    pub fn sensor_needs(&self) -> SensorNeeds {
        SensorNeeds {
            cpu_temp: is_visible_elem(&self.elements, ElementId::CpuTempValue),
            cpu_load: is_visible_elem(&self.elements, ElementId::CpuLoad)
                || is_visible_bar(&self.bars, BarId::CpuLoad),
            ram: is_visible_elem(&self.elements, ElementId::Ram)
                || is_visible_bar(&self.bars, BarId::Ram),
            gpu_temp: is_visible_elem(&self.elements, ElementId::GpuTempValue),
            gpu_load: is_visible_elem(&self.elements, ElementId::GpuLoad)
                || is_visible_bar(&self.bars, BarId::GpuLoad),
            vram: is_visible_elem(&self.elements, ElementId::GpuVram)
                || is_visible_bar(&self.bars, BarId::GpuVram),
            nvme_temp: is_visible_elem(&self.elements, ElementId::NvmeValue),
            fps: is_visible_elem(&self.elements, ElementId::Fps)
                || is_visible_elem(&self.elements, ElementId::Frametime)
                || self.frametime_graph.visible,
        }
    }

    /// Resolve the effective font path for an element.
    /// If the element has a custom font_path, use it; otherwise use the default
    /// bold or regular font based on font_weight.
    pub fn resolve_font_path<'a>(&'a self, elem: &'a ElementConfig) -> &'a str {
        elem.font_path
            .as_deref()
            .unwrap_or_else(|| match elem.font_weight {
                FontWeight::Bold => &self.default_font_bold,
                FontWeight::Regular => &self.default_font_regular,
            })
    }

    /// Get or insert an element config, returning a mutable reference.
    pub fn element_mut(&mut self, id: ElementId) -> &mut ElementConfig {
        if !self.elements.contains_key(&id) {
            let defaults = default_elements();
            self.elements.insert(id, defaults[&id].clone());
        }
        self.elements.get_mut(&id).unwrap()
    }

    /// Get or insert a bar config, returning a mutable reference.
    pub fn bar_mut(&mut self, id: BarId) -> &mut BarConfig {
        if !self.bars.contains_key(&id) {
            let defaults = default_bars();
            self.bars.insert(id, defaults[&id].clone());
        }
        self.bars.get_mut(&id).unwrap()
    }

    /// Get position of a drag target.
    pub fn get_pos(&self, target: &DragTarget) -> (i32, i32) {
        match target {
            DragTarget::Element(id) => {
                if let Some(e) = self.elements.get(id) {
                    (e.x, e.y)
                } else {
                    (0, 0)
                }
            }
            DragTarget::Bar(id) => {
                if let Some(b) = self.bars.get(id) {
                    (b.x, b.y)
                } else {
                    (0, 0)
                }
            }
            DragTarget::Divider => (self.divider.x, self.divider.y_start),
            DragTarget::Gif => (self.gif.x, self.gif.y),
            DragTarget::FrametimeGraph => (self.frametime_graph.x, self.frametime_graph.y),
        }
    }

    /// Set position of a drag target.
    pub fn set_pos(&mut self, target: &DragTarget, x: i32, y: i32) {
        match target {
            DragTarget::Element(id) => {
                if let Some(e) = self.elements.get_mut(id) {
                    e.x = x;
                    e.y = y;
                }
            }
            DragTarget::Bar(id) => {
                if let Some(b) = self.bars.get_mut(id) {
                    b.x = x;
                    b.y = y;
                }
            }
            DragTarget::Divider => {
                self.divider.x = x;
                self.divider.y_start = y;
            }
            DragTarget::Gif => {
                self.gif.x = x;
                self.gif.y = y;
            }
            DragTarget::FrametimeGraph => {
                self.frametime_graph.x = x;
                self.frametime_graph.y = y;
            }
        }
    }
}
