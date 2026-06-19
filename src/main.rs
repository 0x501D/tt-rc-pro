use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use clap::Parser;
use sysinfo::System;

mod app;
mod config;
mod gif;
mod hid;
mod lcd_thread;
mod render;
mod sensor;

// Constants.
const W: u32 = 480;
const H: u32 = 128;
const CHUNK: usize = 1016;
const USB_VID: u16 = 0x264a;
const USB_PID: u16 = 0x232a;

// CLI arguments.
#[derive(Parser, Debug)]
#[command(
    name = "tt-rc-pro",
    about = "Thermaltake RC Pro LCD display controller"
)]
struct Args {
    /// Launch GUI configuration mode.
    #[arg(long)]
    gui: bool,

    /// Run as daemon (default when no --gui).
    #[arg(long)]
    daemon: bool,

    /// Don't send to LCD device (preview only in GUI mode).
    #[arg(long)]
    no_send: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load or create default config.
    let config = config::Config::load().unwrap_or_else(|e| {
        eprintln!("Config load failed ({e}), using defaults");
        config::Config::default()
    });

    if args.gui {
        run_gui(config, args.no_send)
    } else {
        run_daemon(config)
    }
}

fn run_gui(config: config::Config, no_send: bool) -> Result<()> {
    let config = Arc::new(RwLock::new(config));
    let lcd_state = Arc::new(RwLock::new(lcd_thread::LcdState::default()));

    // Find HID device
    let hidraw_path = if no_send {
        None
    } else {
        hid::find_hidraw(USB_VID, USB_PID)
    };

    if hidraw_path.is_some() && !no_send {
        println!(
            "tt-rc-pro GUI: LCD found at {}",
            hidraw_path.as_deref().unwrap()
        );
    } else if !no_send {
        eprintln!("tt-rc-pro GUI: LCD not found (preview-only mode)");
    }

    // Spawn LCD sender thread.
    let shutdown = Arc::new(AtomicBool::new(false));
    let _lcd_handle = lcd_thread::spawn(
        Arc::clone(&config),
        Arc::clone(&lcd_state),
        hidraw_path,
        Arc::clone(&shutdown),
        no_send,
    );

    // Launch eframe GUI.
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 500.0])
            .with_title("Thermaltake RC Pro LCD Config"),
        ..Default::default()
    };

    let config_clone = Arc::clone(&config);
    let lcd_state_clone = Arc::clone(&lcd_state);

    let result = eframe::run_native(
        "tt-rc-pro",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(app::TtRcApp::new(
                cc,
                config_clone,
                lcd_state_clone,
            )))
        }),
    );

    // Signal shutdown.
    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);

    result.map_err(|e| anyhow!("eframe error: {e}"))
}

fn run_daemon(mut config: config::Config) -> Result<()> {
    let hidraw_path = hid::find_hidraw(USB_VID, USB_PID).ok_or_else(|| {
        anyhow!(
            "Thermaltake RC Pro (USB {USB_VID:04x}:{USB_PID:04x}) not found.\n\
             Check udev rules and that the device is connected."
        )
    })?;

    println!(
        "tt-rc-pro daemon: {hidraw_path}  {W}×{H}  update={}s",
        config.update_interval_secs
    );

    let mut font_cache = render::FontCache::new(&config);
    let mut gif = config
        .gif
        .path
        .as_deref()
        .and_then(|p| gif::GifAnimation::load(p).ok());
    let mut last_gif_path = config.gif.path.clone();

    // Warm up sysinfo CPU usage (first refresh returns 0).
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    thread::sleep(Duration::from_millis(500));

    let mut gpu_state = sensor::GpuSensorState::default();
    let mut device: Option<hid::HidDevice> = None;
    let mut first_frame = true;
    let mut consecutive_errors: u32 = 0;
    let mut last_config_mtime: Option<std::time::SystemTime> = None;

    loop {
        // Hot-reload config if file changed.
        let config_path = config::Config::config_path();
        if let Ok(metadata) = std::fs::metadata(&config_path) {
            let mtime = metadata.modified().ok();
            if mtime != last_config_mtime {
                if let Ok(loaded) = config::Config::load() {
                    // Reload GIF if path changed.
                    if loaded.gif.path != last_gif_path {
                        gif = loaded
                            .gif
                            .path
                            .as_deref()
                            .and_then(|p| gif::GifAnimation::load(p).ok());
                        last_gif_path = loaded.gif.path.clone();
                    }
                    // Sync FPS file path.
                    gpu_state.fps_file_path = loaded.fps_file_path.clone();
                    config = loaded;
                    font_cache.reload_defaults(&config);
                    println!("Config reloaded from {}", config_path.display());
                }
                last_config_mtime = mtime;
            }
        }

        match daemon_step(
            &mut device,
            &mut first_frame,
            &mut sys,
            &mut gpu_state,
            &config,
            &mut font_cache,
            gif.as_ref(),
            &hidraw_path,
        ) {
            Ok(()) => {
                consecutive_errors = 0;
                thread::sleep(Duration::from_secs(config.update_interval_secs));
            }
            Err(e) => {
                consecutive_errors += 1;
                eprintln!("\n  Error #{consecutive_errors}: {e}");
                device = None;

                if consecutive_errors >= 2 {
                    eprintln!("  Attempting USB reset...");
                    if let Some((bus, dev)) = hid::find_usb_addr(USB_VID, USB_PID) {
                        if hid::usb_reset(bus, dev) {
                            eprintln!("  Reset bus{bus}/dev{dev:03}, waiting...");
                            consecutive_errors = 0;
                        }
                    } else {
                        eprintln!("  Could not locate USB device for reset");
                        thread::sleep(Duration::from_secs(5));
                    }
                }
            }
        }
    }
}

fn daemon_step(
    device: &mut Option<hid::HidDevice>,
    first_frame: &mut bool,
    sys: &mut System,
    gpu_state: &mut sensor::GpuSensorState,
    config: &config::Config,
    font_cache: &mut render::FontCache,
    gif: Option<&gif::GifAnimation>,
    hidraw_path: &str,
) -> Result<()> {
    // Open device if not already open.
    if device.is_none() {
        if !std::path::Path::new(hidraw_path).exists() {
            return Err(anyhow!("{hidraw_path} not found"));
        }
        *device = Some(hid::HidDevice::open(hidraw_path)?);
        *first_frame = true;
    }
    let dev = device.as_mut().unwrap();

    // Encode JPEG BEFORE any protocol commands (timing-critical after CMD_1D).
    let needs = config.sensor_needs();
    let sensors = sensor::read_sensors(sys, gpu_state, &needs);
    let jpeg = render::make_frame(&sensors, config, font_cache, gif);

    // Send to display.
    if *first_frame {
        dev.init_display()?;
        dev.send_chunks(&jpeg)?;
        *first_frame = false;
    } else {
        dev.begin_next_frame()?;
        dev.send_chunks(&jpeg)?;
    }

    // Status line.
    print!(
        "\r  CPU {}°  GPU {}°  load {:.0}%  GPU {}  FPS {}   ",
        sensors
            .cpu_temp
            .map(|t| format!("{t:.1}"))
            .unwrap_or_else(|| "N/A".into()),
        sensors
            .gpu_temp
            .map(|t| format!("{t:.1}"))
            .unwrap_or_else(|| "N/A".into()),
        sensors.cpu_pct,
        sensors
            .gpu_load_pct
            .map(|p| format!("{p:.0}%"))
            .unwrap_or_else(|| "N/A".into()),
        sensors
            .fps
            .map(|f| format!("{f:.0}"))
            .unwrap_or_else(|| "--".into()),
    );
    std::io::stdout().flush()?;

    Ok(())
}
