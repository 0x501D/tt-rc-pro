use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use sysinfo::System;

use crate::config::SensorNeeds;

/// Sensor data for frame rendering.
#[derive(Debug, Clone, Default)]
pub struct SensorData {
    pub cpu_temp: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub nvme_temp: Option<f32>,
    pub cpu_pct: f32,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub ram_pct: f32,
    // GPU metrics (AMD)
    pub gpu_load_pct: Option<f32>,
    pub vram_used_gb: f64,
    pub vram_total_gb: f64,
    pub vram_pct: f32,
    // FPS / frametime (external file source)
    pub fps: Option<f32>,
    pub frametime_ms: Option<f32>,
    pub frametime_history: Vec<f32>,
}

/// Fixed number of sample slots for the frametime graph.
pub const FRAMETIME_SLOTS: usize = 200;

/// Persistent state for GPU sensor readings across `read_sensors()` calls.
#[derive(Debug)]
pub struct GpuSensorState {
    pub frametime_history: Vec<f32>,
    pub fps_file_path: String,
    max_history: usize,
}

impl Default for GpuSensorState {
    fn default() -> Self {
        GpuSensorState {
            frametime_history: Vec::with_capacity(FRAMETIME_SLOTS),
            fps_file_path: String::from("/tmp/tt-rc-pro-fps"),
            max_history: FRAMETIME_SLOTS,
        }
    }
}

impl GpuSensorState {
    pub fn push_frametime(&mut self, ft: f32) {
        self.frametime_history.push(ft);
        if self.frametime_history.len() > self.max_history {
            self.frametime_history.remove(0);
        }
    }
}

/// Cached AMD GPU PCI device path.
static AMD_PCI_PATH: OnceLock<Option<String>> = OnceLock::new();

/// Detect AMD GPU by scanning /sys/class/drm/renderD*/device/vendor for 0x1002.
/// Returns the canonical PCI path (e.g. /sys/bus/pci/devices/0000:01:00.0).
fn detect_amd_pci_path() -> Option<String> {
    AMD_PCI_PATH
        .get_or_init(|| {
            let entries = fs::read_dir("/sys/class/drm").ok()?;
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if !name_str.starts_with("renderD") {
                    continue;
                }
                let device_path = entry.path().join("device");
                let vendor = fs::read_to_string(device_path.join("vendor")).ok()?;
                if vendor.trim() == "0x1002" {
                    if let Ok(link) = device_path.canonicalize() {
                        return Some(link.to_string_lossy().into_owned());
                    }
                }
            }
            None
        })
        .clone()
}

/// Read GPU load percentage from AMD sysfs.
/// Tries gpu_busy_percent first, then falls back to gpu_metrics binary struct.
fn read_gpu_load(pci_path: &str) -> Option<f32> {
    // Method 1: gpu_busy_percent (simple text sysfs file)
    let busy_path = format!("{pci_path}/gpu_busy_percent");
    if let Ok(val) = fs::read_to_string(&busy_path) {
        if let Ok(pct) = val.trim().parse::<f32>() {
            return Some(pct);
        }
    }

    // Method 2: gpu_metrics binary struct
    let metrics_path = format!("{pci_path}/gpu_metrics");
    if let Ok(data) = fs::read(&metrics_path) {
        if data.len() < 4 {
            return None;
        }
        let _structure_size = u16::from_le_bytes([data[0], data[1]]);
        let format_revision = data[2];

        match format_revision {
            1 => {
                // Desktop dGPU (gpu_metrics_v1_3): average_gfx_activity at offset 10 (u16)
                if data.len() >= 12 {
                    let val = u16::from_le_bytes([data[10], data[11]]);
                    if val != 0xFFFF {
                        return Some(val as f32);
                    }
                }
            }
            2 | 3 => {
                // APU (gpu_metrics_v2_x / v3_0): average_gfx_activity at offset 6 (u16)
                if data.len() >= 8 {
                    let val = u16::from_le_bytes([data[6], data[7]]);
                    if val != 0xFFFF {
                        return Some(val as f32);
                    }
                }
            }
            _ => {}
        }
    }

    None
}

/// Read VRAM total and used from AMD sysfs (bytes → GiB).
fn read_vram(pci_path: &str) -> (f64, f64, f32) {
    let total_path = format!("{pci_path}/mem_info_vram_total");
    let used_path = format!("{pci_path}/mem_info_vram_used");

    let total = fs::read_to_string(&total_path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let used = fs::read_to_string(&used_path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let total_gb = total as f64 / 1_073_741_824.0;
    let used_gb = used as f64 / 1_073_741_824.0;
    let pct = if total > 0 {
        used as f32 / total as f32 * 100.0
    } else {
        0.0
    };

    (used_gb, total_gb, pct)
}

/// Read FPS and frametime from an external file.
/// Format: single line "fps frametime_ms\n"
fn read_fps_file(path: &str) -> (Option<f32>, Option<f32>) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };

    let parts: Vec<&str> = content.trim().split_whitespace().collect();
    if parts.len() >= 2 {
        let fps = parts[0].parse::<f32>().ok();
        let ft = parts[1].parse::<f32>().ok();
        (fps, ft)
    } else if parts.len() == 1 {
        (parts[0].parse::<f32>().ok(), None)
    } else {
        (None, None)
    }
}

/// Read all sensor data: temperatures from /sys/class/hwmon, CPU/RAM from sysinfo,
/// GPU load/VRAM from AMD sysfs, FPS/frametime from external file.
/// Only reads sensors whose corresponding UI elements are visible (per `needs`).
pub fn read_sensors(
    sys: &mut System,
    gpu_state: &mut GpuSensorState,
    needs: &SensorNeeds,
) -> SensorData {
    let (cpu_temp, gpu_temp, nvme_temp) = read_hwmon_temps(needs);

    let cpu_pct = if needs.cpu_load {
        sys.refresh_cpu_usage();
        sys.global_cpu_usage()
    } else {
        0.0
    };

    let (ram_used_gb, ram_total_gb, ram_pct) = if needs.ram {
        sys.refresh_memory();
        let total = sys.total_memory() as f64 / 1_073_741_824.0;
        let used = sys.used_memory() as f64 / 1_073_741_824.0;
        let pct = if total > 0.0 {
            (used / total * 100.0) as f32
        } else {
            0.0
        };
        (used, total, pct)
    } else {
        (0.0, 0.0, 0.0)
    };

    // GPU Load and VRAM (AMD only)
    let (gpu_load_pct, vram_used_gb, vram_total_gb, vram_pct) =
        if let Some(pci_path) = detect_amd_pci_path() {
            let load = if needs.gpu_load {
                read_gpu_load(&pci_path)
            } else {
                None
            };
            let (used, total, pct) = if needs.vram {
                read_vram(&pci_path)
            } else {
                (0.0, 0.0, 0.0)
            };
            (load, used, total, pct)
        } else {
            (None, 0.0, 0.0, 0.0)
        };

    // FPS and frametime from external file
    let (fps, frametime_ms) = if needs.fps {
        let (fps, ft) = read_fps_file(&gpu_state.fps_file_path);
        if let Some(ft_val) = ft {
            gpu_state.push_frametime(ft_val);
        } else {
            // Data source gone — clear history so the graph disappears
            gpu_state.frametime_history.clear();
        }
        (fps, ft)
    } else {
        (None, None)
    };

    SensorData {
        cpu_temp,
        gpu_temp,
        nvme_temp,
        cpu_pct,
        ram_used_gb,
        ram_total_gb,
        ram_pct,
        gpu_load_pct,
        vram_used_gb,
        vram_total_gb,
        vram_pct,
        fps,
        frametime_ms,
        frametime_history: if needs.fps {
            gpu_state.frametime_history.clone()
        } else {
            Vec::new()
        },
    }
}

/// Read CPU (k10temp/Tctl), GPU (amdgpu/edge with high==100), NVMe temperatures
/// from /sys/class/hwmon. Skips device types not needed per `needs`.
fn read_hwmon_temps(needs: &SensorNeeds) -> (Option<f32>, Option<f32>, Option<f32>) {
    let mut cpu_temp = None;
    let mut gpu_temp = None;
    let mut nvme_temp = None;

    // Early return if no temperatures are needed at all.
    if !needs.cpu_temp && !needs.gpu_temp && !needs.nvme_temp {
        return (cpu_temp, gpu_temp, nvme_temp);
    }

    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return (cpu_temp, gpu_temp, nvme_temp);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(name) = fs::read_to_string(path.join("name")) else {
            continue;
        };
        match name.trim() {
            "k10temp" if needs.cpu_temp => cpu_temp = find_temp_by_label(&path, "Tctl"),
            "amdgpu" if needs.gpu_temp => {
                gpu_temp = find_temp_by_label(&path, "edge");
            }
            "nvme" if needs.nvme_temp => nvme_temp = find_temp_by_label(&path, "Composite"),
            _ => {}
        }
    }

    (cpu_temp, gpu_temp, nvme_temp)
}

/// Find a temperature sensor by its label within the given hwmon directory.
fn find_temp_by_label(hwmon_path: &Path, target_label: &str) -> Option<f32> {
    for i in 1.. {
        let label_path = hwmon_path.join(format!("temp{i}_label"));
        if !label_path.exists() {
            break;
        }
        if let Ok(label) = fs::read_to_string(&label_path) {
            if label.trim() == target_label {
                return read_temp_millideg(&hwmon_path.join(format!("temp{i}_input")));
            }
        }
    }
    None
}

/// Read a temperature from a sysfs file (millidegrees → degrees Celsius).
fn read_temp_millideg(path: &Path) -> Option<f32> {
    let val = fs::read_to_string(path).ok()?;
    let millideg: f32 = val.trim().parse().ok()?;
    Some(millideg / 1000.0)
}
