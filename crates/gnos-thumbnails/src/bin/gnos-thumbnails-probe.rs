//! WS-04 probe: capture one window by its Hyprland address and report the
//! thumbnails, for live verification on a throwaway window.
//!
//! usage: gnos-thumbnails-probe ADDRESS MAX_WxMAX_H SECONDS REFRESH_MS [OUT.ppm]
//!
//! Each thumbnail is reported as `GNOS_THUMB_PROBE ready address=… size=…
//! quadrants=[…]` (the mean BGR of each quarter). With OUT, the last one is
//! written as a PPM: only ever point it at a window made for the test.

use gnos_thumbnails::{Event, ThumbnailService, Wanted};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn locked() -> Option<bool> {
    // The lock gate's path, checked without locking the operator's session.
    if std::env::var_os("GNOS_THUMB_PROBE_LOCKED").is_some() {
        return Some(true);
    }
    let id = std::env::var("XDG_SESSION_ID").ok()?;
    let out = std::process::Command::new("loginctl")
        .args(["show-session", &id, "-p", "LockedHint", "--value"])
        .output()
        .ok()?;
    match String::from_utf8_lossy(&out.stdout).trim() {
        "no" => Some(false),
        "yes" => Some(true),
        _ => None,
    }
}

fn quadrants(w: u32, h: u32, bgra: &[u8]) -> Vec<[u32; 3]> {
    let (w, h) = (w as usize, h as usize);
    let mut out = Vec::new();
    for (y0, y1) in [(0, h / 2), (h / 2, h)] {
        for (x0, x1) in [(0, w / 2), (w / 2, w)] {
            // The inner half of each quarter, away from the window's frame.
            let (ix0, ix1) = (x0 + (x1 - x0) / 4, x1 - (x1 - x0) / 4);
            let (iy0, iy1) = (y0 + (y1 - y0) / 4, y1 - (y1 - y0) / 4);
            let mut acc = [0u64; 3];
            let mut n = 0u64;
            for y in iy0..iy1.max(iy0 + 1) {
                for x in ix0..ix1.max(ix0 + 1) {
                    let o = (y * w + x) * 4;
                    for c in 0..3 {
                        acc[c] += u64::from(bgra[o + c]);
                    }
                    n += 1;
                }
            }
            out.push([0, 1, 2].map(|c| (acc[c] / n.max(1)) as u32));
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!("usage: gnos-thumbnails-probe ADDRESS MAX_WxMAX_H SECONDS REFRESH_MS [OUT.ppm]");
        std::process::exit(2);
    }
    let address = u64::from_str_radix(args[1].trim_start_matches("0x"), 16).expect("address");
    let (mw, mh) = args[2].split_once('x').expect("WxH");
    let max = (mw.parse().expect("w"), mh.parse().expect("h"));
    let seconds: u64 = args[3].parse().expect("seconds");
    let refresh = Duration::from_millis(args[4].parse().expect("refresh"));
    let out = args.get(5).cloned();

    let (send, receive) = mpsc::channel();
    let service = ThumbnailService::start(move |e| drop(send.send(e)), locked).expect("start");
    let started = Instant::now();
    service.want(vec![Wanted { address, max }], refresh);
    let mut last = None;
    let mut count = 0;
    while started.elapsed() < Duration::from_secs(seconds) {
        match receive.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Ready {
                address,
                width,
                height,
                bgra,
            }) => {
                count += 1;
                println!(
                    "GNOS_THUMB_PROBE ready address={address:#x} size={width}x{height} after_ms={} quadrants={:?}",
                    started.elapsed().as_millis(),
                    quadrants(width, height, &bgra)
                );
                last = Some((width, height, bgra));
            }
            Ok(other) => println!("GNOS_THUMB_PROBE event={other:?}"),
            Err(_) => {}
        }
    }
    service.want(Vec::new(), refresh);
    println!("GNOS_THUMB_PROBE done count={count}");
    if let (Some(path), Some((w, h, bgra))) = (out, last) {
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        for px in bgra.chunks_exact(4) {
            ppm.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        std::fs::write(&path, ppm).expect("write");
        println!("GNOS_THUMB_PROBE wrote={path}");
    }
}
