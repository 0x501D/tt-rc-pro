use std::fs;
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};

use crate::CHUNK;

//  HID ioctl numbers (linux/hidraw.h)
// _IOC(_IOC_WRITE|_IOC_READ, 'H', nr, size) → direction bits = 3
fn hid_iocsfeature(n: usize) -> u64 {
    (3u64 << 30) | ((n as u64) << 16) | (0x48u64 << 8) | 0x06
}

fn hid_iocgfeature(n: usize) -> u64 {
    (3u64 << 30) | ((n as u64) << 16) | (0x48u64 << 8) | 0x07
}

const USBDEVFS_RESET: u64 = (0u64) | (0u64) | ('U' as u64) << 8 | 20;

// Feature Report payloads (64 bytes each, report ID 0x03).
const CMD_18: [u8; 64] = {
    let mut buf = [0u8; 64];
    buf[0] = 0x03;
    buf[1] = 0x18;
    buf
};

const CMD_1A: [u8; 64] = {
    let mut buf = [0u8; 64];
    buf[0] = 0x03;
    buf[1] = 0x1a;
    buf
};

const CMD_0C_DIMS: [u8; 64] = {
    let mut buf = [0u8; 64];
    buf[0] = 0x03;
    buf[1] = 0x0c;
    buf[2] = 0x64;
    buf[3] = 0x00;
    buf[4] = 0x00;
    buf[5] = 0x00;
    buf[6] = 0x00;
    buf[7] = 0xe0;
    buf[8] = 0x01;
    buf[9] = 0x80;
    buf[10] = 0x00;
    buf[11] = 0x04;
    buf[12] = 0x05;
    buf[13] = 0x00;
    buf[14] = 0x00;
    buf[15] = 0x00;
    buf
};

const CMD_0C_NEXT: [u8; 64] = {
    let mut buf = [0u8; 64];
    buf[0] = 0x03;
    buf[1] = 0x0c;
    buf[2] = 0x64;
    buf[3] = 0xff;
    buf[4] = 0xff;
    buf[5] = 0xea;
    buf[6] = 0xff;
    buf[7] = 0xff;
    buf[8] = 0xff;
    buf
};

const CMD_1D: [u8; 64] = {
    let mut buf = [0u8; 64];
    buf[0] = 0x03;
    buf[1] = 0x1d;
    buf[2] = 0x00;
    buf[3] = 0xff;
    buf[4] = 0xff;
    buf[5] = 0xea;
    buf[6] = 0xff;
    buf[7] = 0xff;
    buf[8] = 0xff;
    buf
};

/// Device discovery.
/// Find /dev/hidraw* path for the given USB VID:PID.
pub fn find_hidraw(vendor: u16, product: u16) -> Option<String> {
    let pattern = format!("{vendor:08X}:{product:08X}");
    let entries = fs::read_dir("/sys/class/hidraw").ok()?;
    for entry in entries.flatten() {
        let uevent_path = entry.path().join("device").join("uevent");
        if let Ok(uevent) = fs::read_to_string(&uevent_path) {
            if uevent.to_uppercase().contains(&pattern) {
                let name = entry.file_name();
                return Some(format!("/dev/{}", name.to_string_lossy()));
            }
        }
    }
    None
}

/// Find (bus, devnum) for USB reset.
pub fn find_usb_addr(vendor: u16, product: u16) -> Option<(u32, u32)> {
    let pattern = format!("{vendor:04x}/{product:04x}");
    let base = Path::new("/sys/bus/usb/devices");
    let entries = fs::read_dir(base).ok()?;
    for entry in entries.flatten() {
        let dev_path = entry.path();
        let uevent_path = dev_path.join("uevent");
        if let Ok(uevent) = fs::read_to_string(&uevent_path) {
            if uevent.contains(&pattern) {
                let bus = fs::read_to_string(dev_path.join("busnum"))
                    .ok()
                    .and_then(|s| s.trim().parse().ok())?;
                let dev = fs::read_to_string(dev_path.join("devnum"))
                    .ok()
                    .and_then(|s| s.trim().parse().ok())?;
                return Some((bus, dev));
            }
        }
    }
    None
}

/// Send USBDEVFS_RESET to recover a NAK-stuck endpoint.
pub fn usb_reset(bus: u32, dev: u32) -> bool {
    let path = format!("/dev/bus/usb/{bus:03}/{dev:03}");
    match fs::OpenOptions::new().write(true).open(&path) {
        Ok(file) => {
            let ret = unsafe { libc::ioctl(file.as_raw_fd(), USBDEVFS_RESET as _, 0) };
            if ret < 0 {
                eprintln!("  USB reset ioctl failed ({path})");
                false
            } else {
                thread::sleep(Duration::from_secs(1));
                true
            }
        }
        Err(e) => {
            eprintln!("  USB reset failed ({path}): {e}");
            false
        }
    }
}

/// HID device wrapper.
pub struct HidDevice {
    file: std::fs::File,
}

impl HidDevice {
    pub fn open(path: &str) -> Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| anyhow!("Cannot open {path}: {e}"))?;
        Ok(Self { file })
    }

    fn raw_fd(&self) -> libc::c_int {
        self.file.as_raw_fd()
    }

    fn set_feature(&self, data: &[u8]) -> Result<()> {
        let request = hid_iocsfeature(data.len());
        let mut buf = data.to_vec();
        let ret = unsafe { libc::ioctl(self.raw_fd(), request as libc::c_ulong, buf.as_mut_ptr()) };
        if ret < 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }

    fn get_feature(&self, report_id: u8) -> Result<[u8; 64]> {
        let mut buf = [0u8; 64];
        buf[0] = report_id;
        let request = hid_iocgfeature(64);
        let ret = unsafe { libc::ioctl(self.raw_fd(), request as libc::c_ulong, buf.as_mut_ptr()) };
        if ret < 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(buf)
    }

    /// One-time init sequence. Must be followed immediately by send_chunks() —
    /// any gap after CMD_1D causes ETIMEDOUT.
    pub fn init_display(&mut self) -> Result<()> {
        print!("  [init] ");
        std::io::stdout().flush()?;

        self.set_feature(&CMD_18)?;
        thread::sleep(Duration::from_millis(50));
        print!("18 ");
        std::io::stdout().flush()?;

        self.set_feature(&CMD_1A)?;
        thread::sleep(Duration::from_millis(50));
        print!("1a ");
        std::io::stdout().flush()?;

        match self.get_feature(0x07) {
            Ok(r) => print!(
                "rpt07={:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x} ",
                r[1], r[2], r[3], r[4], r[5], r[6], r[7], r[8], r[9], r[10], r[11], r[12]
            ),
            Err(e) => print!("(rpt07:{e}) "),
        }
        std::io::stdout().flush()?;

        for _ in 0..4 {
            self.set_feature(&CMD_0C_DIMS)?;
            thread::sleep(Duration::from_millis(20));
        }
        print!("0c×4 ");
        std::io::stdout().flush()?;

        match self.get_feature(0x0f) {
            Ok(r) => print!("rpt0f={:#04x} ", r[1]),
            Err(e) => print!("(rpt0f:{e}) "),
        }
        std::io::stdout().flush()?;

        self.set_feature(&CMD_1D)?;
        print!("1d→");
        std::io::stdout().flush()?;

        Ok(())
    }

    /// Per-frame handshake for frame 2 onwards.
    pub fn begin_next_frame(&mut self) -> Result<()> {
        // Ignore errors from get_feature — just proceed
        let _ = self.get_feature(0x0f);
        self.set_feature(&CMD_0C_NEXT)?;
        thread::sleep(Duration::from_millis(20));
        Ok(())
    }

    /// Write JPEG data as HID Output Report 2 chunks.
    ///
    /// Chunk header (8 bytes):
    ///   byte 0:   0x02        Report ID
    ///   byte 1:   0x09        constant
    ///   byte 2:   0x00        constant
    ///   byte 3:   0x00/0x01   0=normal, 1=last chunk (triggers display render)
    ///   bytes 4-5 LE uint16   1016 for normal; actual trailing bytes for last
    ///   bytes 6-7 LE uint16   chunk index (0-based)
    pub fn send_chunks(&mut self, jpeg: &[u8]) -> Result<()> {
        let pad = (CHUNK - jpeg.len() % CHUNK) % CHUNK;
        let mut data = jpeg.to_vec();
        data.extend(std::iter::repeat(0u8).take(pad));

        let total_chunks = data.len() / CHUNK;

        for (i, chunk) in data.chunks(CHUNK).enumerate() {
            let is_last = i == total_chunks - 1;
            let mut hdr = [0u8; 8];
            hdr[0] = 0x02;
            hdr[1] = 0x09;
            hdr[2] = 0x00;
            if is_last {
                let trailing = jpeg.len() - i * CHUNK;
                hdr[3] = 0x01;
                hdr[4] = (trailing & 0xff) as u8;
                hdr[5] = ((trailing >> 8) & 0xff) as u8;
            } else {
                hdr[3] = 0x00;
                hdr[4] = 0xf8; // 1016 = 0x03f8
                hdr[5] = 0x03;
            }
            hdr[6] = (i & 0xff) as u8;
            hdr[7] = ((i >> 8) & 0xff) as u8;

            let mut report = Vec::with_capacity(8 + CHUNK);
            report.extend_from_slice(&hdr);
            report.extend_from_slice(chunk);
            self.file.write_all(&report)?;
        }

        Ok(())
    }
}
