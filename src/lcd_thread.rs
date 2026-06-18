use std::sync::Arc;
use std::sync::RwLock;
use std::thread;
use std::time::Duration;

use sysinfo::System;

use crate::config::Config;
use crate::gif::GifAnimation;
use crate::hid;
use crate::render::{self, FontCache};
use crate::sensor;
use crate::{USB_PID, USB_VID};

/// Shared state between GUI and LCD thread.
pub struct LcdState {
    pub last_sensor_data: sensor::SensorData,
    pub device_connected: bool,
    pub consecutive_errors: u32,
}

impl Default for LcdState {
    fn default() -> Self {
        LcdState {
            last_sensor_data: sensor::SensorData::default(),
            device_connected: false,
            consecutive_errors: 0,
        }
    }
}

/// Spawn the LCD sender background thread.
/// Returns a JoinHandle that will terminate when `shutdown` is set.
pub fn spawn(
    config: Arc<RwLock<Config>>,
    lcd_state: Arc<RwLock<LcdState>>,
    hidraw_path: Option<String>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    no_send: bool,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        thread::sleep(Duration::from_millis(500));

        let mut device: Option<hid::HidDevice> = None;
        let mut first_frame = true;

        // Initialize font cache and GIF from current config.
        let cfg = config.read().unwrap().clone();
        let mut font_cache = FontCache::new(&cfg);
        let mut gif = cfg
            .gif
            .path
            .as_deref()
            .and_then(|p| GifAnimation::load(p).ok());
        let mut last_gif_path = cfg.gif.path.clone();

        while !shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            // Read sensors.
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let sensor_data = sensor::read_sensors(&mut sys);

            // Update shared state for GUI preview.
            {
                let mut state = lcd_state.write().unwrap();
                state.last_sensor_data = sensor_data.clone();
            }

            // Check if GIF path changed.
            {
                let cfg = config.read().unwrap();
                let current_path = cfg.gif.path.clone();
                if current_path != last_gif_path {
                    gif = current_path
                        .as_deref()
                        .and_then(|p| GifAnimation::load(p).ok());
                    last_gif_path = current_path;
                }
            }

            if no_send {
                // Preview-only mode, just wait.
                let interval = {
                    let cfg = config.read().unwrap();
                    cfg.update_interval_secs
                };
                thread::sleep(Duration::from_secs(interval));
                continue;
            }

            // Render JPEG.
            let jpeg = {
                let cfg = config.read().unwrap();
                render::make_frame(&sensor_data, &cfg, &mut font_cache, gif.as_ref())
            };

            // Send to LCD.
            if let Some(ref path) = hidraw_path {
                let result = send_to_device(&mut device, &mut first_frame, &jpeg, path);
                match result {
                    Ok(()) => {
                        let mut state = lcd_state.write().unwrap();
                        state.device_connected = true;
                        state.consecutive_errors = 0;
                    }
                    Err(e) => {
                        eprintln!("\n  LCD Error: {e}");
                        device = None;
                        let mut state = lcd_state.write().unwrap();
                        state.consecutive_errors += 1;

                        if state.consecutive_errors >= 2 {
                            eprintln!("  Attempting USB reset...");
                            if let Some((bus, dev)) = hid::find_usb_addr(USB_VID, USB_PID) {
                                if hid::usb_reset(bus, dev) {
                                    eprintln!("  Reset bus{bus}/dev{dev:03}, waiting...");
                                    state.consecutive_errors = 0;
                                }
                            } else {
                                eprintln!("  Could not locate USB device for reset");
                                thread::sleep(Duration::from_secs(5));
                            }
                        }
                    }
                }
            }

            // Wait for next update.
            let interval = {
                let cfg = config.read().unwrap();
                cfg.update_interval_secs
            };
            thread::sleep(Duration::from_secs(interval));
        }

        eprintln!("LCD thread shutting down.");
    })
}

fn send_to_device(
    device: &mut Option<hid::HidDevice>,
    first_frame: &mut bool,
    jpeg: &[u8],
    hidraw_path: &str,
) -> anyhow::Result<()> {
    // Open device if not already open.
    if device.is_none() {
        if !std::path::Path::new(hidraw_path).exists() {
            return Err(anyhow::anyhow!("{hidraw_path} not found"));
        }
        *device = Some(hid::HidDevice::open(hidraw_path)?);
        *first_frame = true;
    }
    let dev = device.as_mut().unwrap();

    if *first_frame {
        dev.init_display()?;
        dev.send_chunks(jpeg)?;
        *first_frame = false;
    } else {
        dev.begin_next_frame()?;
        dev.send_chunks(jpeg)?;
    }

    Ok(())
}
