# tt-rc-pro

Linux controller for the **Thermaltake RC Pro** LCD display — the built-in screen found on Thermaltake Tower series PC cases. Renders system telemetry (temperatures, CPU load, RAM usage, time/date) onto a 480×128 JPEG and pushes it to the LCD panel over USB HID.

Generated with GLM-5.1 based on https://github.com/pcmx1/thermaltake-lcd-linux

## Features

- **Daemon mode** — headless loop that reads sensors, renders frames, and drives the LCD
- **GUI mode** — live preview with drag-and-drop element positioning, color/font/size pickers, save/load/reset
- **Hot-reload config** — daemon watches `config.toml` and picks up changes on the fly
- **Auto-recovery** — USB errors trigger a bus reset and reconnect
- **Dynamic colors** — temperature values shift green → yellow → red based on configurable thresholds
- **Progress bars** — CPU load and RAM usage bars with customizable fill/background/border colors
- **GIF overlay** — animated GIF composited on top of the frame with alpha blending
- **Preview-only mode** — `--no-send` runs the GUI without needing the physical LCD

## Screenshots

*TODO: add screenshots of the GUI and LCD output*

## Requirements

- Linux (uses `/sys/class/hwmon`, `/sys/class/hidraw`, and Linux `ioctl`)
- Thermaltake RC Pro LCD connected via USB (VID `0x264a`, PID `0x232a`)
- Read/write access to `/dev/hidraw*` (add a udev rule or run as root)
- DejaVu Sans fonts at `/usr/share/fonts/TTF` (configurable)

## Build

```bash
cargo build --release
```

## Usage

```bash
# Daemon mode (default) — headless, drives the LCD
tt-rc-pro

# GUI mode — interactive configuration with live preview
tt-rc-pro --gui

# Preview only — no LCD hardware needed
tt-rc-pro --gui --no-send
```

## Configuration

Config file: `~/.config/tt-rc-pro/config.toml`

Auto-created with defaults on first run. The daemon hot-reloads when the file changes; the GUI auto-saves on exit.

### Global settings

| Setting | Default | Description |
|---|---|---|
| `background` | `#000000` | Frame background color |
| `bold_font` | `/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf` | Path to bold font |
| `regular_font` | `/usr/share/fonts/dejavu/DejaVuSans.ttf` | Path to regular font |
| `update_interval_secs` | `2` | Seconds between frame updates (1–30) |
| `jpeg_quality` | `92` | JPEG encoding quality (10–100) |

### Display elements

All elements support `x`, `y` positioning and `visible` toggle. Text elements also support `font_size`, `bold`, and `color`.

| Element | Type | Description |
|---|---|---|
| `CpuTempLabel` | text | "CPU TEMP" label |
| `CpuTempValue` | text | CPU temperature (dynamic color by threshold) |
| `CpuLoad` | text | CPU load percentage |
| `Ram` | text | RAM usage ("12.3/32 GB") |
| `GpuTempLabel` | text | "GPU TEMP" label |
| `GpuTempValue` | text | GPU temperature (dynamic color) |
| `NvmeLabel` | text | "NVME" label |
| `NvmeValue` | text | NVMe temperature (dynamic color) |
| `Time` | text | Current time (HH:MM:SS) |
| `Date` | text | Current date (MM/DD) |
| `CpuLoadBar` | bar | CPU load progress bar |
| `RamBar` | bar | RAM usage progress bar |
| `Divider` | divider | Vertical separator line |
| `Gif` | gif | Animated GIF overlay |

### Bar elements

Bars support `width`, `height`, `fill_color`, `bg_color`, and `border_color`.

### GIF element

| Setting | Description |
|---|---|
| `path` | Path to the GIF file |
| `x`, `y` | Position on the frame |
| `display_width`, `display_height` | Scaled dimensions (omit for original size) |

### Example config

```toml
background = "#000000"
update_interval_secs = 2
jpeg_quality = 92

[CpuTempLabel]
x = 10
y = 5
font_size = 14
bold = true
color = "#ffffff"
visible = true

[CpuTempValue]
x = 10
y = 22
font_size = 22
bold = true
color = "#00ff00"
dynamic_color = true
visible = true

[CpuLoadBar]
x = 10
y = 50
width = 200
height = 10
fill_color = "#00ff00"
bg_color = "#333333"
border_color = "#666666"
visible = true

[Divider]
x = 250
y_start = 5
y_end = 123
color = "#444444"
visible = true

[Gif]
path = "/path/to/animation.gif"
x = 350
y = 10
display_width = 120
display_height = 60
visible = true
```

## Tech stack

| Layer | Technology |
|---|---|
| Language | Rust (edition 2021) |
| GUI | eframe 0.31 + egui 0.31 |
| Image rendering | image 0.25 + imageproc 0.25 + ab_glyph 0.2 |
| System sensors | sysinfo 0.33 + `/sys/class/hwmon` |
| USB/HID | libc 0.2 (ioctl) + `/dev/hidraw*` |
| Config | serde + toml |
| CLI | clap 4 |

## Sensor support

| Sensor | Source | Notes |
|---|---|---|
| CPU temp | `k10temp` → label "Tctl" | AMD |
| GPU temp | `amdgpu` → label "edge" | AMD |
| NVMe temp | `nvme` → label "Composite" | |
| CPU load | `sysinfo` crate | |
| RAM usage | `sysinfo` crate | |

## USB HID protocol

- Device discovered by scanning `/sys/class/hidraw` for VID `0x264a` / PID `0x232a`
- Feature reports: 64-byte payloads, report ID `0x03` via `HIDIOCSFEATURE`/`HIDIOCGFEATURE`
- Init sequence: `CMD_18` → `CMD_1A` → read report `0x07` → `CMD_0C_DIMS` (×4) → read report `0x0F` → `CMD_1D`
- JPEG transfer: Output Report 2 chunks (8-byte header + 1016 bytes payload); last chunk flagged `0x01` triggers display
- On repeated errors: `USBDEVFS_RESET` ioctl to reset the device
