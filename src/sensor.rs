use std::fs;
use std::path::Path;

use sysinfo::System;

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
}

/// Read all sensor data: temperatures from /sys/class/hwmon, CPU/RAM from sysinfo.
pub fn read_sensors(sys: &mut System) -> SensorData {
    let (cpu_temp, gpu_temp, nvme_temp) = read_hwmon_temps();

    sys.refresh_cpu_usage();
    let cpu_pct = sys.global_cpu_usage();

    sys.refresh_memory();
    let ram_total = sys.total_memory() as f64 / 1_073_741_824.0; // bytes → GB
    let ram_used = sys.used_memory() as f64 / 1_073_741_824.0;
    let ram_pct = if ram_total > 0.0 {
        (ram_used / ram_total * 100.0) as f32
    } else {
        0.0
    };

    SensorData {
        cpu_temp,
        gpu_temp,
        nvme_temp,
        cpu_pct,
        ram_used_gb: ram_used,
        ram_total_gb: ram_total,
        ram_pct,
    }
}

/// Read CPU (k10temp/Tctl), GPU (amdgpu/edge with high==100), NVMe temperatures
/// from /sys/class/hwmon.
fn read_hwmon_temps() -> (Option<f32>, Option<f32>, Option<f32>) {
    let mut cpu_temp = None;
    let mut gpu_temp = None;
    let mut nvme_temp = None;

    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return (cpu_temp, gpu_temp, nvme_temp);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(name) = fs::read_to_string(path.join("name")) else {
            continue;
        };
        match name.trim() {
            "k10temp" => cpu_temp = find_temp_by_label(&path, "Tctl"),
            "amdgpu" => {
                gpu_temp = find_temp_by_label(&path, "edge");
            }
            "nvme" => nvme_temp = find_temp_by_label(&path, "Composite"),
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
