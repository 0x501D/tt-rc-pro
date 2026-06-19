# tt-rc-pro

Linux controller for the **Thermaltake RC Pro** LCD display — the built-in screen found on Thermaltake Tower series PC cases. Renders system telemetry (temperatures, CPU load, RAM usage, GPU load, VRAM, FPS, frametime, time/date) onto a 480×128 JPEG and pushes it to the LCD panel over USB HID.

Generated with GLM-5.1 based on https://github.com/pcmx1/thermaltake-lcd-linux

## Features

- **Daemon mode** — headless loop that reads sensors, renders frames, and drives the LCD
- **GUI mode** — live preview with drag-and-drop element positioning, color/font/size pickers, save/load/reset
- **Hot-reload config** — daemon watches `config.toml` and picks up changes on the fly
- **Auto-recovery** — USB errors trigger a bus reset and reconnect
- **Dynamic colors** — temperature values shift green → yellow → red based on configurable thresholds
- **Progress bars** — CPU load, RAM usage, GPU load, and VRAM usage bars with customizable fill/background/border colors
- **GPU metrics** — GPU load and VRAM usage for AMD GPUs (sysfs + gpu_metrics binary)
- **FPS & frametime** — reads FPS and frametime from an external file (e.g. MangoHud output), with a live frametime line graph
- **GIF overlay** — animated GIF composited on top of the frame with alpha blending
- **Preview-only mode** — `--no-send` runs the GUI without needing the physical LCD

## Screenshots

<img width="600" alt="image" src="https://github.com/user-attachments/assets/2c3e4489-6add-4162-8230-c02fe04a7153" />
<img width="600" alt="image" src="https://github.com/user-attachments/assets/4605ee7f-b45d-4306-817c-3e3e2d694c75" />

## Requirements

- Linux (uses `/sys/class/hwmon`, `/sys/class/drm`, `/sys/class/hidraw`, and Linux `ioctl`)
- Thermaltake RC Pro LCD connected via USB (VID `0x264a`, PID `0x232a`)
- Read/write access to `/dev/hidraw*` (add a udev rule or run as root)
- DejaVu Sans fonts at `/usr/share/fonts/TTF` (configurable)
- **For GPU metrics**: AMD GPU with `amdgpu` driver (reads from `/sys/class/drm/renderD*/device/`)
- **For FPS/frametime**: an external data source writing to a file (see [FPS/frametime setup](#fps--frametime-setup))

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
| `fps_file_path` | `/tmp/tt-rc-pro-fps` | Path to file providing FPS/frametime data |

### Display elements

All elements support `x`, `y` positioning and `visible` toggle. Text elements also support `font_size`, `font_weight`, and `color`.

| Element | Type | Description |
|---|---|---|
| `CpuTempLabel` | text | "CPU TEMP" label |
| `CpuTempValue` | text | CPU temperature (dynamic color by threshold) |
| `CpuLoad` | text | CPU load percentage |
| `Ram` | text | RAM usage ("12.3/32 GB") |
| `GpuTempLabel` | text | "GPU TEMP" label |
| `GpuTempValue` | text | GPU temperature (dynamic color) |
| `GpuLoad` | text | GPU load percentage ("LOAD 67%") |
| `GpuVram` | text | VRAM usage ("VRAM 4.2/8.0 GB") |
| `Fps` | text | Frames per second ("FPS 144") |
| `Frametime` | text | Frametime in ms ("FT 6.9ms") |
| `NvmeLabel` | text | "NVME" label |
| `NvmeValue` | text | NVMe temperature (dynamic color) |
| `Time` | text | Current time (HH:MM:SS) |
| `Date` | text | Current date (MM/DD) |

### Bar elements

Bars support `width`, `height`, `fill_color`, `bg_color`, and `border_color`.

| Bar | Default fill | Description |
|---|---|---|
| `CpuLoad` | Blue (`#3377ff`) | CPU load progress bar |
| `Ram` | Purple (`#8844cc`) | RAM usage progress bar |
| `GpuLoad` | Orange (`#ff8833`) | GPU load progress bar |
| `GpuVram` | Pink (`#cc4488`) | VRAM usage progress bar |

### Frametime graph

The frametime graph is a configurable line graph that shows frametime history. Peaks (stutter) are clearly visible as spikes.

| Setting | Default | Description |
|---|---|---|
| `visible` | `true` | Show/hide the graph |
| `x`, `y` | `248`, `112` | Position on the frame |
| `width`, `height` | `222`, `14` | Graph dimensions in pixels |
| `line_color` | `#4488ff` | Color of the frametime line |
| `bg_color` | `#1a1a1a` | Background fill |
| `border_color` | `#333333` | Border outline |
| `max_ms` | `0` | Y-axis max in ms (0 = auto-scale from data) |

### GIF element

| Setting | Description |
|---|---|
| `path` | Path to the GIF file |
| `x`, `y` | Position on the frame |
| `display_width`, `display_height` | Scaled dimensions (omit for original size) |

### Example config

```toml
background_color = [0, 0, 0]
update_interval_secs = 2
jpeg_quality = 92
fps_file_path = "/tmp/tt-rc-pro-fps"

[CpuTempLabel]
x = 10
y = 5
font_size = 14
font_weight = "Bold"
color = [255, 255, 255]
visible = true

[CpuTempValue]
x = 10
y = 22
font_size = 22
font_weight = "Bold"
color = [0, 255, 0]
use_dynamic_color = true
visible = true

[elements.GpuLoad]
x = 248
y = 58
font_size = 16
color = [170, 170, 170]
visible = true

[bars.GpuLoad]
x = 248
y = 74
width = 222
height = 10
fill_color = [255, 136, 51]
bg_color = [26, 26, 26]
border_color = [51, 51, 51]
visible = true

[frametime_graph]
visible = true
x = 248
y = 112
width = 222
height = 14
line_color = [68, 136, 255]
bg_color = [26, 26, 26]
border_color = [51, 51, 51]
max_ms = 0.0

[Divider]
x = 250
y_start = 5
y_end = 123
color = [68, 68, 68]
visible = true

[Gif]
path = "/path/to/animation.gif"
x = 350
y = 10
visible = true
```

## FPS & frametime setup

FPS and frametime data comes from an **external file** that another program writes to. The default path is `/tmp/tt-rc-pro-fps` (configurable via `fps_file_path`).

### File format

A single line with two space-separated values: `fps frametime_ms`

```
144 6.94
```

### Using with MangoHud

MangoHud can output metrics to a log file. Create a small script that tails the log and writes the latest values:

```bash
#!/bin/bash
# mango-to-ttrc.sh — pipe MangoHud output to tt-rc-pro
MANGO_LOG="/tmp/mangohud.log"
FPS_FILE="/tmp/tt-rc-pro-fps"

tail -F "$MANGO_LOG" | while read -r line; do
    # Extract FPS and frametime from MangoHud log format
    fps=$(echo "$line" | grep -oP 'fps=\K[0-9.]+')
    ft=$(echo "$line" | grep -oP 'frametime=\K[0-9.]+')
    if [ -n "$fps" ] && [ -n "$ft" ]; then
        echo "$fps $ft" > "$FPS_FILE"
    fi
done
```

### Using with a custom script

```bash
# Simple test: write static FPS/frametime
echo "60 16.67" > /tmp/tt-rc-pro-fps
```

## Tech stack

| Layer | Technology |
|---|---|
| Language | Rust (edition 2021) |
| GUI | eframe 0.31 + egui 0.31 |
| Image rendering | image 0.25 + imageproc 0.25 + ab_glyph 0.2 |
| System sensors | sysinfo 0.33 + `/sys/class/hwmon` + `/sys/class/drm` |
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
| GPU load | `amdgpu` → `gpu_busy_percent` or `gpu_metrics` binary | AMD only |
| VRAM usage | `amdgpu` → `mem_info_vram_total` / `mem_info_vram_used` | AMD only |
| FPS | External file (`fps_file_path`) | Requires external data source |
| Frametime | External file (`fps_file_path`) | Requires external data source |

## USB HID protocol

- Device discovered by scanning `/sys/class/hidraw` for VID `0x264a` / PID `0x232a`
- Feature reports: 64-byte payloads, report ID `0x03` via `HIDIOCSFEATURE`/`HIDIOCGFEATURE`
- Init sequence: `CMD_18` → `CMD_1A` → read report `0x07` → `CMD_0C_DIMS` (×4) → read report `0x0F` → `CMD_1D`
- JPEG transfer: Output Report 2 chunks (8-byte header + 1016 bytes payload); last chunk flagged `0x01` triggers display
- On repeated errors: `USBDEVFS_RESET` ioctl to reset the device
