//! What this computer offers local speech recognition: installed memory and
//! a dedicated GPU. It decides the default recognizer and the Nemotron build:
//!
//! - 8 GB of memory or more (the minimum): Nemotron; less: Parakeet;
//! - a dedicated card with 2 GB of video memory or more: CUDA on NVIDIA,
//!   Vulkan on AMD and Intel; otherwise the CPU build (Metal on Apple
//!   silicon when timed faster).
//!
//! Recommended: 16 GB of memory and a dedicated card with 4 GB.
//!
//! Read once, from the system (registry on Windows, `/sys` and `/proc` on
//! Linux, `sysctl` on macOS), without starting anything from the models
//! folder. When a GPU build later fails to start, the CPU build still runs.

use std::sync::OnceLock;

use crate::{NemotronBackend, SttEngine};

/// Minimum memory for Nemotron: 8 GB. Linux reports memory without what the
/// firmware and an integrated GPU keep (up to about 2 GB on an 8 GB laptop),
/// so the line sits under 8 GiB; 4 GB and 6 GB computers stay below.
const NEMOTRON_MIN_RAM: u64 = 6 * 1024 * 1024 * 1024 + 1;
/// Video memory a card needs to run Nemotron on it: 2 GB (the model takes
/// about 1.5 GB). Unknown amounts count for NVIDIA and Intel Arc cards;
/// `nemo-speech doctor` still checks the free memory before starting.
const DEDICATED_MIN_VRAM: u64 = 2 * 1024 * 1024 * 1024;

const VENDOR_NVIDIA: u16 = 0x10de;
const VENDOR_AMD: u16 = 0x1002;
const VENDOR_INTEL: u16 = 0x8086;

/// Maker of a dedicated GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
}

/// Memory and dedicated GPU of this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechHardware {
    /// Installed memory; `None` when the system did not say.
    pub ram_bytes: Option<u64>,
    pub dedicated_gpu: Option<GpuVendor>,
}

impl SpeechHardware {
    /// This computer, read once.
    pub fn detect() -> Self {
        static DETECTED: OnceLock<SpeechHardware> = OnceLock::new();
        *DETECTED.get_or_init(|| {
            let hardware = Self {
                ram_bytes: ram_bytes(),
                dedicated_gpu: dedicated_gpu(),
            };
            tracing::info!(
                ram_mb = hardware.ram_bytes.map(|b| b / (1024 * 1024)),
                gpu = ?hardware.dedicated_gpu,
                "speech hardware"
            );
            hardware
        })
    }

    /// The recognizer to use when the student has not chosen one.
    pub fn recommended_stt(self) -> SttEngine {
        let nemotron_here = crate::nemotron::runtime_package(NemotronBackend::Cpu).is_some();
        if nemotron_here && self.runs_nemotron() {
            SttEngine::Nemotron
        } else {
            SttEngine::Parakeet
        }
    }

    fn runs_nemotron(self) -> bool {
        self.ram_bytes.is_some_and(|b| b >= NEMOTRON_MIN_RAM)
    }

    /// The Nemotron build for this card: CUDA on NVIDIA, Vulkan on AMD and
    /// Intel. `None` without a dedicated card of 2 GB, where the builds are
    /// timed (on PCs only the CPU build qualifies then).
    pub fn preferred_backend(self) -> Option<NemotronBackend> {
        match self.dedicated_gpu? {
            GpuVendor::Nvidia => Some(NemotronBackend::Cuda),
            GpuVendor::Amd | GpuVendor::Intel => Some(NemotronBackend::Gpu),
        }
    }
}

/// A display adapter as the system lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Adapter {
    vendor: u16,
    device: u16,
    name: String,
    vram: Option<u64>,
}

impl Adapter {
    /// The maker, when this is a dedicated card with 2 GB of its own. NVIDIA
    /// makes only dedicated cards for PCs; Radeon integrated graphics are
    /// named "... Graphics" (their memory is a share of the system's);
    /// Intel's dedicated cards are the Arc A and B series.
    fn dedicated(&self) -> Option<GpuVendor> {
        let room = |unknown_ok: bool| self.vram.map_or(unknown_ok, |v| v >= DEDICATED_MIN_VRAM);
        match self.vendor {
            VENDOR_NVIDIA => room(true).then_some(GpuVendor::Nvidia),
            VENDOR_AMD => {
                let integrated_name = self.name.trim().to_lowercase().ends_with("graphics");
                (room(false) && !integrated_name).then_some(GpuVendor::Amd)
            }
            VENDOR_INTEL => (intel_arc_card(self.device) && room(true)).then_some(GpuVendor::Intel),
            _ => None,
        }
    }
}

/// PCI device ids of Intel's dedicated cards: Alchemist (0x56xx) and
/// Battlemage (0xe20x, 0xe21x).
fn intel_arc_card(device: u16) -> bool {
    device >> 8 == 0x56 || device >> 4 == 0xe20 || device >> 4 == 0xe21
}

/// The best dedicated GPU among `adapters`: NVIDIA, then AMD, then Intel.
fn best_dedicated(adapters: &[Adapter]) -> Option<GpuVendor> {
    let found: Vec<GpuVendor> = adapters.iter().filter_map(Adapter::dedicated).collect();
    [GpuVendor::Nvidia, GpuVendor::Amd, GpuVendor::Intel]
        .into_iter()
        .find(|vendor| found.contains(vendor))
}

fn dedicated_gpu() -> Option<GpuVendor> {
    let adapters = adapters();
    // No adapter list (unusual system): an NVIDIA driver still means a card.
    if adapters.is_empty() && crate::nemotron::nvidia_driver_present() {
        return Some(GpuVendor::Nvidia);
    }
    best_dedicated(&adapters)
}

/// `pci\ven_1002&dev_7340&...` → `(0x1002, 0x7340)`.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn parse_pci_id(id: &str) -> Option<(u16, u16)> {
    let id = id.to_ascii_lowercase();
    let hex = |tag: &str| {
        let at = id.find(tag)? + tag.len();
        u16::from_str_radix(id.get(at..at + 4)?, 16).ok()
    };
    Some((hex("ven_")?, hex("dev_")?))
}

#[cfg(windows)]
fn adapters() -> Vec<Adapter> {
    windows::adapters()
}

#[cfg(windows)]
fn ram_bytes() -> Option<u64> {
    windows::ram_bytes()
}

#[cfg(target_os = "linux")]
fn adapters() -> Vec<Adapter> {
    use std::fs;
    let read_hex = |path: std::path::PathBuf| {
        let text = fs::read_to_string(path).ok()?;
        u16::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
    };
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix("card")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|entry| {
            let dir = entry.path().join("device");
            Some(Adapter {
                vendor: read_hex(dir.join("vendor"))?,
                device: read_hex(dir.join("device"))?,
                name: String::new(),
                vram: fs::read_to_string(dir.join("mem_info_vram_total"))
                    .ok()
                    .and_then(|text| text.trim().parse().ok()),
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn ram_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// Macs have no dedicated GPU that a Nemotron build uses.
#[cfg(not(any(windows, target_os = "linux")))]
fn adapters() -> Vec<Adapter> {
    Vec::new()
}

#[cfg(target_os = "macos")]
fn ram_bytes() -> Option<u64> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn ram_bytes() -> Option<u64> {
    None
}

#[cfg(windows)]
mod windows {
    //! Installed memory and the display adapters in the registry.
    #![allow(unsafe_code)]

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_ANY, RegGetValueW};
    use windows_sys::Win32::System::SystemInformation::{
        GetPhysicallyInstalledSystemMemory, GlobalMemoryStatusEx, MEMORYSTATUSEX,
    };

    use super::Adapter;

    /// Display adapters' driver keys.
    const DISPLAY_CLASS: &str =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

    pub(super) fn ram_bytes() -> Option<u64> {
        let mut kib: u64 = 0;
        // SAFETY: writes one u64 owned here.
        if unsafe { GetPhysicallyInstalledSystemMemory(&mut kib) } != 0 && kib > 0 {
            return Some(kib * 1024);
        }
        let mut status = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: `status` is a MEMORYSTATUSEX with its length set.
        (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullTotalPhys)
    }

    pub(super) fn adapters() -> Vec<Adapter> {
        (0..32)
            .filter_map(|index| {
                let key = format!(r"{DISPLAY_CLASS}\{index:04}");
                let id = text(&value(&key, "MatchingDeviceId")?);
                let (vendor, device) = super::parse_pci_id(&id)?;
                let name = value(&key, "DriverDesc").map_or_else(String::new, |v| text(&v));
                let vram = value(&key, "HardwareInformation.qwMemorySize")
                    .or_else(|| value(&key, "HardwareInformation.MemorySize"))
                    .and_then(|v| number(&v));
                Some(Adapter {
                    vendor,
                    device,
                    name,
                    vram,
                })
            })
            .collect()
    }

    /// Raw bytes of a value under `HKEY_LOCAL_MACHINE\key`.
    fn value(key: &str, name: &str) -> Option<Vec<u8>> {
        let key = wide(key);
        let name = wide(name);
        let mut size: u32 = 0;
        // SAFETY: null-terminated strings; a null buffer asks only for the size.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_ANY,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS || size == 0 || size > 64 * 1024 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        // SAFETY: `buffer` holds `size` bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_ANY,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        buffer.truncate(size as usize);
        Some(buffer)
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A REG_SZ value.
    fn text(bytes: &[u8]) -> String {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .take_while(|&unit| unit != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }

    /// A REG_QWORD, REG_DWORD or 4-byte REG_BINARY value.
    fn number(bytes: &[u8]) -> Option<u64> {
        match bytes.len() {
            8 => Some(u64::from_le_bytes(bytes.try_into().ok()?)),
            4 => Some(u64::from(u32::from_le_bytes(bytes.try_into().ok()?))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn adapter(vendor: u16, device: u16, name: &str, vram: Option<u64>) -> Adapter {
        Adapter {
            vendor,
            device,
            name: name.into(),
            vram,
        }
    }

    /// Manual: `cargo test -p kg-audio --lib this_computer -- --ignored --nocapture`.
    #[test]
    #[ignore = "prints this computer's hardware"]
    fn this_computer() {
        let hardware = SpeechHardware::detect();
        println!("{hardware:?} -> {:?}", hardware.recommended_stt());
        println!("{:?}", adapters());
    }

    #[test]
    fn reads_pci_ids() {
        assert_eq!(
            parse_pci_id(r"PCI\VEN_1002&DEV_7340&SUBSYS_0B0C1002"),
            Some((0x1002, 0x7340))
        );
        assert_eq!(parse_pci_id("pci\\ven_10de"), None);
    }

    #[test]
    fn tells_dedicated_cards_from_integrated_graphics() {
        let rx5500 = adapter(VENDOR_AMD, 0x7340, "AMD Radeon RX 5500", Some(8 * GIB));
        let apu = adapter(
            VENDOR_AMD,
            0x15bf,
            "AMD Radeon(TM) 780M Graphics",
            Some(4 * GIB),
        );
        let small_apu = adapter(VENDOR_AMD, 0x1638, "", Some(512 * 1024 * 1024));
        let uhd = adapter(VENDOR_INTEL, 0x9a49, "Intel(R) UHD Graphics", Some(GIB / 8));
        let arc = adapter(VENDOR_INTEL, 0x56a0, "Intel(R) Arc(TM) A770 Graphics", None);
        let b580 = adapter(VENDOR_INTEL, 0xe20b, "Intel(R) Arc(TM) B580 Graphics", None);
        let rtx = adapter(VENDOR_NVIDIA, 0x2504, "NVIDIA GeForce RTX 3060", None);
        assert_eq!(rx5500.dedicated(), Some(GpuVendor::Amd));
        assert_eq!(apu.dedicated(), None);
        assert_eq!(small_apu.dedicated(), None);
        assert_eq!(uhd.dedicated(), None);
        assert_eq!(arc.dedicated(), Some(GpuVendor::Intel));
        assert_eq!(b580.dedicated(), Some(GpuVendor::Intel));
        assert_eq!(rtx.dedicated(), Some(GpuVendor::Nvidia));
        // Cards under 2 GB run on the CPU.
        let gt710 = adapter(VENDOR_NVIDIA, 0x128b, "NVIDIA GeForce GT 710", Some(GIB));
        let gtx1050 = adapter(
            VENDOR_NVIDIA,
            0x1c81,
            "NVIDIA GeForce GTX 1050",
            Some(2 * GIB),
        );
        let rx550 = adapter(VENDOR_AMD, 0x699f, "Radeon RX 550 Series", Some(2 * GIB));
        assert_eq!(gt710.dedicated(), None);
        assert_eq!(gtx1050.dedicated(), Some(GpuVendor::Nvidia));
        assert_eq!(rx550.dedicated(), Some(GpuVendor::Amd));
        assert_eq!(
            best_dedicated(&[uhd.clone(), rx5500, rtx]),
            Some(GpuVendor::Nvidia)
        );
        assert_eq!(best_dedicated(&[uhd, apu]), None);
    }

    #[test]
    fn nemotron_needs_8_gb_of_memory() {
        let hardware = |ram_gib: Option<u64>, gpu| SpeechHardware {
            ram_bytes: ram_gib.map(|g| g * GIB),
            dedicated_gpu: gpu,
        };
        assert!(!hardware(Some(4), None).runs_nemotron());
        assert!(!hardware(Some(6), None).runs_nemotron());
        assert!(!hardware(Some(4), Some(GpuVendor::Nvidia)).runs_nemotron());
        assert!(!hardware(None, None).runs_nemotron());
        // An 8 GB laptop whose integrated GPU keeps 1 GB.
        let laptop = SpeechHardware {
            ram_bytes: Some(7 * GIB - 200 * 1024 * 1024),
            dedicated_gpu: None,
        };
        assert!(laptop.runs_nemotron());
        assert!(hardware(Some(8), None).runs_nemotron());
        assert!(hardware(Some(16), Some(GpuVendor::Amd)).runs_nemotron());
        assert_eq!(
            hardware(Some(8), Some(GpuVendor::Nvidia)).preferred_backend(),
            Some(NemotronBackend::Cuda)
        );
        assert_eq!(
            hardware(Some(8), Some(GpuVendor::Amd)).preferred_backend(),
            Some(NemotronBackend::Gpu)
        );
        assert_eq!(
            hardware(Some(8), Some(GpuVendor::Intel)).preferred_backend(),
            Some(NemotronBackend::Gpu)
        );
        assert_eq!(hardware(Some(32), None).preferred_backend(), None);
    }
}
