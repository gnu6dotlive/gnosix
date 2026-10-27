//! The dashboard's facts (SURF-03A): resources and the system, read only.
//!
//! - CPU: `/proc/stat`'s aggregate line, as a share of busy time between two
//!   reads.
//! - Memory: `/proc/meminfo`'s `MemTotal` and `MemAvailable`.
//! - Disk: `statvfs` of the root filesystem.
//! - Temperature: the CPU package sensor under `/sys/class/hwmon`
//!   (`coretemp`, `k10temp` or `zenpower`; `acpitz` otherwise).
//! - System: the user, the host name, the kernel release and the uptime.
//!
//! Every reader takes its root, so tests (and the qualification) can hand it
//! a stand-in tree. Nothing here writes.

use std::path::Path;

/// Cumulative CPU time, in jiffies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

/// Parse `/proc/stat`'s aggregate `cpu` line.
pub fn parse_proc_stat(text: &str) -> Option<CpuTimes> {
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .map(|f| f.parse().ok())
        .collect::<Option<_>>()?;
    if fields.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest is in user).
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    let total: u64 = fields.iter().take(8).sum();
    Some(CpuTimes {
        busy: total.saturating_sub(idle),
        total,
    })
}

/// The busy share between two reads, in percent; `None` without progress.
pub fn cpu_percent(before: CpuTimes, after: CpuTimes) -> Option<f32> {
    let total = after.total.checked_sub(before.total)?;
    let busy = after.busy.checked_sub(before.busy)?;
    (total > 0).then(|| (busy as f32 / total as f32 * 100.0).clamp(0.0, 100.0))
}

/// Used and total bytes of something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Usage {
    pub used: u64,
    pub total: u64,
}

impl Usage {
    pub fn percent(self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f32 / self.total as f32 * 100.0).clamp(0.0, 100.0)
        }
    }
}

/// Parse `/proc/meminfo`: used = total − available.
pub fn parse_meminfo(text: &str) -> Option<Usage> {
    let field = |name: &str| -> Option<u64> {
        let line = text.lines().find(|l| l.starts_with(name))?;
        line.split_whitespace().nth(1)?.parse::<u64>().ok()
    };
    let total = field("MemTotal:")? * 1024;
    let available = field("MemAvailable:")? * 1024;
    Some(Usage {
        used: total.saturating_sub(available),
        total,
    })
}

/// The root filesystem's usage (`statvfs`).
pub fn read_disk(mount: &Path) -> Option<Usage> {
    let stat = rustix::fs::statvfs(mount).ok()?;
    let block = stat.f_frsize;
    let total = stat.f_blocks * block;
    let free = stat.f_bfree * block;
    Some(Usage {
        used: total.saturating_sub(free),
        total,
    })
}

/// The CPU package temperature in °C, from a hwmon tree.
pub fn read_cpu_temperature(hwmon: &Path) -> Option<f32> {
    let mut best: Option<(u8, f32)> = None;
    for entry in std::fs::read_dir(hwmon).ok()?.flatten() {
        let dir = entry.path();
        let name = std::fs::read_to_string(dir.join("name")).unwrap_or_default();
        let rank = match name.trim() {
            "coretemp" | "k10temp" | "zenpower" => 0,
            "acpitz" => 1,
            _ => continue,
        };
        let Some(milli) = std::fs::read_to_string(dir.join("temp1_input"))
            .ok()
            .and_then(|t| t.trim().parse::<i64>().ok())
        else {
            continue;
        };
        let celsius = milli as f32 / 1000.0;
        if best.is_none_or(|(r, _)| rank < r) {
            best = Some((rank, celsius));
        }
    }
    best.map(|(_, c)| c)
}

/// The system, as the dashboard's profile card shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemFacts {
    pub user: Option<String>,
    pub host: Option<String>,
    pub kernel: Option<String>,
    pub uptime_secs: Option<u64>,
}

/// Parse `/proc/uptime`'s first field.
pub fn parse_uptime(text: &str) -> Option<u64> {
    text.split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
        .map(|s| s as u64)
}

/// "3 d 4 h", "4 h 12 min", "12 min".
pub fn format_uptime(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60);
    if d > 0 {
        format!("{d} d {h} h")
    } else if h > 0 {
        format!("{h} h {m} min")
    } else {
        format!("{m} min")
    }
}

/// Read the system facts under `proc` (normally `/proc`).
pub fn read_system(proc: &Path) -> SystemFacts {
    let read = |p: &str| {
        std::fs::read_to_string(proc.join(p))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    SystemFacts {
        user: std::env::var("USER").ok().filter(|u| !u.is_empty()),
        host: read("sys/kernel/hostname"),
        kernel: read("sys/kernel/osrelease"),
        uptime_secs: read("uptime").and_then(|t| parse_uptime(&t)),
    }
}

/// One read of the resources.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResourceSample {
    pub cpu_times: Option<CpuTimes>,
    pub memory: Option<Usage>,
    pub disk: Option<Usage>,
    pub temperature: Option<f32>,
}

/// Read the resources under `proc` and `hwmon`, and the disk at `mount`.
pub fn read_resources(proc: &Path, hwmon: &Path, mount: &Path) -> ResourceSample {
    ResourceSample {
        cpu_times: std::fs::read_to_string(proc.join("stat"))
            .ok()
            .and_then(|t| parse_proc_stat(&t)),
        memory: std::fs::read_to_string(proc.join("meminfo"))
            .ok()
            .and_then(|t| parse_meminfo(&t)),
        disk: read_disk(mount),
        temperature: read_cpu_temperature(hwmon),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_share_is_busy_time_between_two_reads() {
        let a = parse_proc_stat("cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 1 2 3 4\n").unwrap();
        assert_eq!(
            a,
            CpuTimes {
                busy: 150,
                total: 1000
            }
        );
        let b = parse_proc_stat("cpu  200 0 100 850 50 0 0 0 0 0\n").unwrap();
        assert_eq!(cpu_percent(a, b), Some(75.0));
        assert_eq!(cpu_percent(a, a), None);
        assert_eq!(parse_proc_stat("intr 1 2 3"), None);
    }

    #[test]
    fn memory_is_total_less_available() {
        let m =
            parse_meminfo("MemTotal:       16000 kB\nMemFree: 1000 kB\nMemAvailable:    4000 kB\n")
                .unwrap();
        assert_eq!(
            m,
            Usage {
                used: 12000 * 1024,
                total: 16000 * 1024
            }
        );
        assert_eq!(m.percent(), 75.0);
        assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
    }

    #[test]
    fn the_package_sensor_wins_over_acpi() {
        let root = std::env::temp_dir().join(format!("gnos-hwmon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (dir, name, milli) in [
            ("hwmon0", "acpitz", "27800"),
            ("hwmon6", "coretemp", "52000"),
            ("hwmon1", "nvme", "40000"),
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join("name"), format!("{name}\n")).unwrap();
            std::fs::write(root.join(dir).join("temp1_input"), format!("{milli}\n")).unwrap();
        }
        assert_eq!(read_cpu_temperature(&root), Some(52.0));
        std::fs::remove_dir_all(root.join("hwmon6")).unwrap();
        assert_eq!(read_cpu_temperature(&root), Some(27.8));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn uptime_reads_and_formats() {
        assert_eq!(parse_uptime("12345.67 9999.00\n"), Some(12345));
        assert_eq!(format_uptime(12345), "3 h 25 min");
        assert_eq!(format_uptime(3 * 86_400 + 4 * 3600 + 5), "3 d 4 h");
        assert_eq!(format_uptime(600), "10 min");
    }

    #[test]
    fn the_root_filesystem_reads() {
        let disk = read_disk(Path::new("/")).expect("statvfs /");
        assert!(disk.total > 0 && disk.used <= disk.total);
    }
}
