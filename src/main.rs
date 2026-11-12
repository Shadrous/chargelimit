//! chargelimit: set the Apple Silicon firmware charge limit and exit.
//!
//! The SMC firmware shipped with macOS 15.8 enforces a charge window itself
//! (keys bfF0 / bfD0 / bfE0), so no resident daemon is needed: write the
//! limits once and the firmware stops charging at `upper` and resumes below
//! `lower`, even while the Mac sleeps.

mod smc;

use smc::Smc;
use std::process::exit;

const ACTIVE: &str = "bfF0"; // ui8: 0x00 off, 0x02 on
const UPPER: &str = "bfD0"; // ui32, little-endian percent
const LOWER: &str = "bfE0"; // ui32, little-endian percent
const BATTERY_PCT: &str = "BUIC"; // ui8
const BATTERY_MA: &str = "B0AC"; // si16 little-endian, + charging / - discharging
const AC_PRESENT: &str = "AC-W"; // si8, > 0 when an adapter is connected

extern "C" {
    fn geteuid() -> u32;
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  chargelimit status\n  sudo chargelimit set <upper> <lower>   e.g. set 80 70 (saved, re-applied at boot)\n  sudo chargelimit off                   normal charging (saved)\n  sudo chargelimit apply                 re-apply the saved setting (run at boot)"
    );
    exit(2)
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("chargelimit: {msg}");
    exit(1)
}

fn need_root() {
    if unsafe { geteuid() } != 0 {
        fail("this command must be run as root (use sudo)");
    }
}

fn read_u32le(smc: &Smc, key: &str) -> u32 {
    let k = smc.read(key).unwrap_or_else(|e| fail(format!("read {key}: {e}")));
    u32::from_le_bytes(k.bytes[..4].try_into().unwrap())
}

fn read_u8(smc: &Smc, key: &str) -> Option<u8> {
    smc.read(key).ok().and_then(|k| k.bytes.first().copied())
}

fn write(smc: &Smc, key: &str, bytes: &[u8]) {
    smc.write(key, bytes).unwrap_or_else(|e| fail(format!("write {key}: {e}")));
}

fn status(smc: &Smc) {
    let active = read_u8(smc, ACTIVE) == Some(0x02);
    let (upper, lower) = (read_u32le(smc, UPPER), read_u32le(smc, LOWER));
    let pct = read_u8(smc, BATTERY_PCT).map_or("?".into(), |p| format!("{p}%"));
    let ac = read_u8(smc, AC_PRESENT).is_some_and(|v| (v as i8) > 0);
    let ma = smc.read(BATTERY_MA).ok().map(|k| i16::from_le_bytes([k.bytes[0], k.bytes[1]]));
    let flow = match ma {
        Some(m) if m > 50 => format!("charging ({m} mA)"),
        Some(m) if m < -50 => format!("discharging ({m} mA)"),
        Some(_) => "not charging".into(),
        None => "?".into(),
    };
    println!("battery: {pct}, {flow}, adapter {}", if ac { "connected" } else { "not connected" });
    if active {
        println!("limit:   on (stop at {upper}%, resume below {lower}%)");
    } else {
        println!("limit:   off");
    }
}

fn set(smc: &Smc, upper: u32, lower: u32) {
    if !(20..=100).contains(&upper) || !(10..upper).contains(&lower) {
        fail("need 20 <= upper <= 100 and 10 <= lower < upper");
    }
    // Firmware requires: deactivate, write upper, write lower, activate.
    write(smc, ACTIVE, &[0x00]);
    write(smc, UPPER, &upper.to_le_bytes());
    write(smc, LOWER, &lower.to_le_bytes());
    write(smc, ACTIVE, &[0x02]);
}

const CONF: &str = "/usr/local/etc/chargelimit.conf"; // "80 70" or "off"

fn save(setting: &str) {
    let _ = std::fs::create_dir_all("/usr/local/etc");
    if let Err(e) = std::fs::write(CONF, format!("{setting}\n")) {
        eprintln!("chargelimit: warning: could not save {CONF}: {e}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let smc = Smc::open().unwrap_or_else(|e| fail(e));
    if smc.read(ACTIVE).is_err() {
        fail("this Mac's firmware has no firmware charge-limit keys (bfF0/bfD0/bfE0)");
    }
    let parse = |s: &str| s.trim_end_matches('%').parse::<u32>().ok();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] | ["status"] => status(&smc),
        ["set", u, l] => {
            need_root();
            let (Some(u), Some(l)) = (parse(u), parse(l)) else { usage() };
            set(&smc, u, l);
            save(&format!("{u} {l}"));
            status(&smc);
        }
        ["off"] => {
            need_root();
            write(&smc, ACTIVE, &[0x00]);
            save("off");
            status(&smc);
        }
        ["apply"] => {
            need_root();
            let conf = std::fs::read_to_string(CONF).unwrap_or_default();
            let words: Vec<&str> = conf.split_whitespace().collect();
            match words.as_slice() {
                ["off"] => write(&smc, ACTIVE, &[0x00]),
                [u, l] => match (parse(u), parse(l)) {
                    (Some(u), Some(l)) => set(&smc, u, l),
                    _ => fail(format!("bad config in {CONF}: {conf:?}")),
                },
                _ => fail(format!("no saved setting in {CONF}")),
            }
        }
        _ => usage(),
    }
}
