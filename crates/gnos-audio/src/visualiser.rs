//! SURF-04B (W04): the audio visualiser's source, read only.
//!
//! A capture stream on the default sink's monitor (`stream.capture.sink`),
//! passive so it never keeps the sink running, on its own thread. Every
//! `HOP` frames, the latest `FFT_SIZE` mono samples become `BANDS`
//! log-spaced band levels (0..=1, from −70 dB to full scale) through a
//! Hann-windowed radix-2 FFT. A silent window yields `None`. Nothing here
//! changes any sound: it only reads the monitor.
//!
//! The session manager links the stream to the monitor (WirePlumber honours
//! `stream.capture.sink`); without one (a private test server) the stream
//! waits until something links it.

use pipewire as pw;
use pw::{properties::properties, spa};
use std::sync::mpsc;

pub const FFT_SIZE: usize = 1024;
pub const BANDS: usize = 32;
const LOW_HZ: f32 = 40.0;
const HIGH_HZ: f32 = 16_000.0;
const FLOOR_DB: f32 = -70.0;
const SILENCE_RMS: f32 = 1e-4;
pub const VISUALISER_NODE: &str = "gnosix-visualiser";

/// In-place radix-2 FFT; `re` and `im` have the same power-of-two length.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let next = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = next;
            }
        }
        len <<= 1;
    }
}

/// The frequency range of band `i` of `n`, log-spaced.
pub fn band_range(i: usize, n: usize, rate: f32) -> (f32, f32) {
    let high = HIGH_HZ.min(rate / 2.0);
    let ratio = (high / LOW_HZ).powf(1.0 / n as f32);
    (
        LOW_HZ * ratio.powi(i as i32),
        LOW_HZ * ratio.powi(i as i32 + 1),
    )
}

/// Band levels of the last `FFT_SIZE` samples, or `None` when they are
/// silent.
pub fn bands(samples: &[f32], rate: f32, n: usize) -> Option<Vec<f32>> {
    let window = &samples[samples.len().saturating_sub(FFT_SIZE)..];
    if window.len() < FFT_SIZE {
        return None;
    }
    let rms = (window.iter().map(|s| s * s).sum::<f32>() / FFT_SIZE as f32).sqrt();
    if rms < SILENCE_RMS {
        return None;
    }
    let mut re: Vec<f32> = window
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let hann =
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos();
            s * hann
        })
        .collect();
    let mut im = vec![0.0; FFT_SIZE];
    fft(&mut re, &mut im);
    let bin_hz = rate / FFT_SIZE as f32;
    // A full-scale sine through a Hann window peaks near N/4.
    let full = FFT_SIZE as f32 / 4.0;
    Some(
        (0..n)
            .map(|b| {
                let (lo, hi) = band_range(b, n, rate);
                let first = ((lo / bin_hz).floor() as usize).max(1);
                let last = ((hi / bin_hz).ceil() as usize).clamp(first + 1, FFT_SIZE / 2);
                let peak = (first..last)
                    .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt())
                    .fold(0.0f32, f32::max);
                let db = 20.0 * (peak / full).max(1e-9).log10();
                ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
            })
            .collect(),
    )
}

/// Stops the capture thread when dropped.
pub struct VisualiserHandle {
    stop: pw::channel::Sender<()>,
}

impl Drop for VisualiserHandle {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

/// Start capturing the default sink's monitor; `on_bands` gets the band
/// levels about 30 times a second (`None` while silent).
pub fn spawn_visualiser<F>(on_bands: F) -> std::io::Result<VisualiserHandle>
where
    F: Fn(Option<Vec<f32>>) + Send + 'static,
{
    let (stop_tx, stop_rx) = pw::channel::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    std::thread::Builder::new()
        .name("gnos-visualiser".into())
        .spawn(move || {
            if let Err(error) = run(on_bands, stop_rx, &ready_tx) {
                let _ = ready_tx.send(Err(error));
            }
        })?;
    match ready_rx.recv_timeout(std::time::Duration::from_secs(3)) {
        Ok(Ok(())) => Ok(VisualiserHandle { stop: stop_tx }),
        Ok(Err(error)) => Err(std::io::Error::other(error)),
        Err(_) => Err(std::io::Error::other("visualiser did not start")),
    }
}

struct Capture<F> {
    rate: f32,
    channels: usize,
    samples: Vec<f32>,
    since: usize,
    on_bands: F,
}

fn run<F>(
    on_bands: F,
    stop: pw::channel::Receiver<()>,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<(), String>
where
    F: Fn(Option<Vec<f32>>) + Send + 'static,
{
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| e.to_string())?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(|e| e.to_string())?;
    let core = context.connect_rc(None).map_err(|e| e.to_string())?;
    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::STREAM_CAPTURE_SINK => "true",
        *pw::keys::NODE_NAME => VISUALISER_NODE,
        *pw::keys::NODE_PASSIVE => "true",
        *pw::keys::APP_NAME => "Gnosix visualiser",
    };
    let stream =
        pw::stream::StreamBox::new(&core, VISUALISER_NODE, props).map_err(|e| e.to_string())?;
    let capture = Capture {
        rate: 48_000.0,
        channels: 2,
        samples: Vec::with_capacity(FFT_SIZE * 2),
        since: 0,
        on_bands,
    };
    let _listener = stream
        .add_local_listener_with_user_data(capture)
        .param_changed(|_, capture, id, param| {
            let Some(param) = param else {
                return;
            };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let mut info = spa::param::audio::AudioInfoRaw::new();
            if info.parse(param).is_ok() {
                capture.rate = info.rate().max(1) as f32;
                capture.channels = info.channels().max(1) as usize;
            }
        })
        .process(|stream, capture| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else {
                return;
            };
            let size = data.chunk().size() as usize;
            let Some(bytes) = data.data() else {
                return;
            };
            let floats = bytes[..size.min(bytes.len())]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            let channels = capture.channels;
            let mut frame = 0.0;
            for (i, s) in floats.enumerate() {
                frame += s;
                if i % channels == channels - 1 {
                    capture.samples.push(frame / channels as f32);
                    frame = 0.0;
                    capture.since += 1;
                }
            }
            if capture.samples.len() > FFT_SIZE * 2 {
                let excess = capture.samples.len() - FFT_SIZE;
                capture.samples.drain(..excess);
            }
            let hop = (capture.rate / 30.0) as usize;
            if capture.since >= hop {
                capture.since = 0;
                (capture.on_bands)(bands(&capture.samples, capture.rate, BANDS));
            }
        })
        .register()
        .map_err(|e| e.to_string())?;
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    let object = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map_err(|e| format!("{e:?}"))?
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&values).ok_or("format pod")?];
    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| e.to_string())?;
    let _stop = stop.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });
    let _ = ready.send(Ok(()));
    mainloop.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One FFT bin at 48 kHz: the lowest bands are narrower than a bin, so a
    /// tone may light the band beside its own.
    const BIN_HZ: f32 = 48_000.0 / FFT_SIZE as f32;

    fn tone(hz: f32, amplitude: f32) -> Vec<f32> {
        (0..FFT_SIZE)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    #[test]
    fn the_fft_finds_a_tone_at_its_bin() {
        let mut re: Vec<f32> = (0..64)
            .map(|i| (2.0 * std::f32::consts::PI * 5.0 * i as f32 / 64.0).cos())
            .collect();
        let mut im = vec![0.0; 64];
        fft(&mut re, &mut im);
        let mags: Vec<f32> = (0..32)
            .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt())
            .collect();
        let peak = mags
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert_eq!(peak, 5);
        assert!((mags[5] - 32.0).abs() < 1e-3);
    }

    #[test]
    fn a_tone_lights_its_band_and_silence_none() {
        for hz in [220.0, 1000.0, 5000.0] {
            let levels = bands(&tone(hz, 0.5), 48_000.0, BANDS).expect("not silent");
            let peak = levels
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap()
                .0;
            let (lo, hi) = band_range(peak, BANDS, 48_000.0);
            assert!(
                lo - BIN_HZ <= hz && hz <= hi + BIN_HZ,
                "{hz} Hz peaked in band {peak} ({lo:.0}..{hi:.0})"
            );
            assert!(levels[peak] > 0.8, "{hz}: {}", levels[peak]);
        }
        assert_eq!(bands(&vec![0.0; FFT_SIZE], 48_000.0, BANDS), None);
        assert_eq!(
            bands(&tone(1000.0, 0.5)[..100], 48_000.0, BANDS),
            None,
            "too short"
        );
    }

    #[test]
    fn bands_are_log_spaced_and_contiguous() {
        let (lo, _) = band_range(0, BANDS, 48_000.0);
        let (_, hi) = band_range(BANDS - 1, BANDS, 48_000.0);
        assert!((lo - 40.0).abs() < 1e-3 && (hi - 16_000.0).abs() < 1.0);
        for i in 1..BANDS {
            assert!(
                (band_range(i - 1, BANDS, 48_000.0).1 - band_range(i, BANDS, 48_000.0).0).abs()
                    < 1e-2
            );
        }
    }
}
