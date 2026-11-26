use anyhow::{Context, Result};
use divegrader_core::{apply_filter, get_filter_matrix_from_rgb, interpolate_matrices, GradeConfig, Matrix};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::path::Path;

pub fn is_video(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref(),
        Some("mp4") | Some("mkv") | Some("mov") | Some("avi") | Some("webm")
    )
}

struct Probe { width: u32, height: u32, fps: f64, has_audio: bool }

fn probe(input: &str) -> Result<Probe> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate", "-show_entries", "stream=codec_name", "-of", "json", input])
        .output().context("ffprobe")?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).context("parse ffprobe")?;
    let s = &v["streams"][0];
    let w = s["width"].as_u64().context("width")? as u32;
    let h = s["height"].as_u64().context("height")? as u32;
    let fr = s["r_frame_rate"].as_str().unwrap_or("30/1");
    let fps = parse_rate(fr);
    // audio check
    let aout = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0", input])
        .output().context("ffprobe audio")?;
    let has_audio = !aout.stdout.is_empty();
    Ok(Probe { width: w, height: h, fps, has_audio })
}

fn parse_rate(s: &str) -> f64 {    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let a: f64 = parts[0].parse().unwrap_or(30.0);
        let b: f64 = parts[1].parse().unwrap_or(1.0);
        if b != 0.0 { a / b } else { a }
    } else {
        s.parse().unwrap_or(30.0)
    }
}

fn use_nvenc(hw: &str) -> bool {
    if hw == "cpu" { return false; }
    if hw == "nvenc" { return true; }
    // auto: check ffmpeg for nvenc
    Command::new("ffmpeg").args(["-hide_banner", "-encoders"]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("hevc_nvenc"))
        .unwrap_or(false)
}

fn decoder(input: &str, hw: &str) -> Result<std::process::Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error"]);
    if hw != "cpu" {
        cmd.args(["-hwaccel", "cuda"]);
    }
    cmd.args(["-i", input, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdout(Stdio::piped());
    cmd.spawn().context("spawn ffmpeg decoder")
}

pub fn process_video(input: &Path, output: &Path, cfg: &GradeConfig, cli: &crate::cli::Cli, progress: Option<std::sync::mpsc::Sender<crate::tui::ProgressEvent>>, name: &str, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<()> {
    let input_s = input.to_string_lossy().to_string();
    let t0 = std::time::Instant::now();
    let probe = probe(&input_s)?;
    let w = probe.width;
    let h = probe.height;
    let fps = probe.fps;
    let frame_bytes = (w as usize) * (h as usize) * 3;

    // stabilization analysis pass (vidstabdetect)
    let trf_path = if cli.stabilize {
        let p = std::env::temp_dir().join(format!("divegrader_{}.trf", std::process::id()));
        let vf = format!("scale=640:360,vidstabdetect=shakiness=5:accuracy=15:result={}", p.display());
        let status = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-i", &input_s, "-vf", &vf, "-f", "null", "-"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("vidstabdetect")?;
        if !status.success() {
            anyhow::bail!("vidstabdetect failed");
        }
        Some(p)
    } else {
        None
    };

    // Pass 1 (sampling only; decode scaled down for speed)
    let mut child = if cli.hw != "cpu" {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-hwaccel", "cuda", "-i", &input_s, "-vf", "scale=256:256", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .stdout(Stdio::piped())
            .spawn()
            .context("spawn ffmpeg pass1 decoder")?
    } else {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-i", &input_s, "-vf", "scale=256:256", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .stdout(Stdio::piped())
            .spawn()
            .context("spawn ffmpeg pass1 decoder")?
    };
    let mut stdout = child.stdout.take().context("decoder stdout")?;
    let small_bytes = 256 * 256 * 3;
    let mut buf = vec![0u8; small_bytes];
    let mut samples: Vec<(usize, Matrix)> = Vec::new();
    let interval = (fps * cli.sample_seconds as f64).round().max(1.0) as usize;
    let mut frame_count = 0usize;
    let mut last_sampled: Option<Vec<u8>> = None;
    loop {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = child.kill();
            return Ok(());
        }
        if stdout.read_exact(&mut buf).is_err() { break; }
        let scene_change = match &last_sampled {
            Some(prev) => mean_abs_diff(prev, &buf) > 30.0,
            None => true,
        };
        if frame_count % interval == 0 || scene_change {
            let m = get_filter_matrix_from_rgb(256, 256, &buf, cfg);
            samples.push((frame_count, m));
            last_sampled = Some(buf.clone());
        }
        frame_count += 1;
    }
    let _ = child.wait();
    if cancel.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    if frame_count == 0 {
        anyhow::bail!("no frames decoded");
    }

    let samples = smooth_samples(samples);
    let matrices = interpolate_matrices(&samples, frame_count);
    if std::env::var("DG_TIMING").is_ok() { eprintln!("pass1 done: {} frames, {} samples, {:?}", frame_count, samples.len(), t0.elapsed()); }
    let t_after_pass1 = std::time::Instant::now();

    // Pass 2
    let mut dec = decoder(&input_s, &cli.hw)?;
    let mut dec_out = dec.stdout.take().context("decoder stdout")?;

    let nvenc = use_nvenc(&cli.hw);
    let codec_ffmpeg = if nvenc {
        match cli.codec.as_str() {
            "x264" | "h264" => "h264_nvenc",
            _ => "hevc_nvenc",
        }
    } else {
        match cli.codec.as_str() {
            "x264" => "libx264",
            "x265" => "libx265",
            "vp9" => "libvpx-vp9",
            other => other,
        }
    };

    let mut enc_args: Vec<String> = vec![
        "-y".into(),
        "-hide_banner".into(), "-loglevel".into(), "error".into(),
        "-f".into(), "rawvideo".into(), "-pix_fmt".into(), "rgb24".into(),
        "-s".into(), format!("{}x{}", w, h),
        "-r".into(), format!("{:.6}", fps),
        "-i".into(), "-".into(),
        "-i".into(), input_s.clone(),
    ];
    if cli.stabilize {
        if let Some(trf) = &trf_path {
            enc_args.extend([
                "-filter_complex".into(),
                format!("[0:v]vidstabtransform=input={}:zoom=5:smoothing=15[vo]", trf.display()),
                "-map".into(), "[vo]".into(),
            ]);
        } else {
            enc_args.extend(["-map".into(), "0:v:0".into()]);
        }
    } else {
        enc_args.extend(["-map".into(), "0:v:0".into()]);
    }
    if !cli.no_audio && probe.has_audio {
        enc_args.extend(["-map".into(), "1:a?".into()]);
    }
    enc_args.extend([
        "-c:v".into(), codec_ffmpeg.into(),
    ]);
    if nvenc {
        enc_args.extend(["-rc".into(), "vbr".into(), "-cq".into(), cli.crf.to_string()]);
    } else {
        enc_args.extend(["-crf".into(), cli.crf.to_string()]);
    }
    enc_args.extend([
        "-preset".into(), cli.preset.clone(),
    ]);
    if !cli.no_audio && probe.has_audio {
        enc_args.extend(["-c:a".into(), "copy".into()]);
    }
    enc_args.extend([
        "-map_metadata".into(), "1".into(),
        "-movflags".into(), "+faststart".into(),
        output.to_string_lossy().to_string(),
    ]);

    let mut enc = Command::new("ffmpeg")
        .args(&enc_args)
        .stdin(Stdio::piped())
        .spawn()
        .context("spawn ffmpeg encoder")?;
    let mut enc_in = enc.stdin.take().context("encoder stdin")?;

    let frame_bytes = w as usize * h as usize * 3;
    let (tx_raw, rx_raw) = crossbeam_channel::bounded::<(usize, Vec<u8>)>(8);
    let (tx_done, rx_done) = crossbeam_channel::bounded::<(usize, Vec<u8>)>(8);

    let nworkers = if cli.gpu { 2 } else { 4 };
    let mut worker_handles = Vec::new();
    for _ in 0..nworkers {
        let rx = rx_raw.clone();
        let tx = tx_done.clone();
        let ww = w as usize;
        let hh = h as usize;
        let mats = matrices.clone();
        let cfg2 = cfg.clone();
        let gpu = cli.gpu;
        let cancel2 = cancel.clone();
        worker_handles.push(std::thread::spawn(move || {
            while let Ok(item) = rx.recv() {
                let (idx, mut frame): (usize, Vec<u8>) = item;
                if cancel2.load(std::sync::atomic::Ordering::SeqCst) { break; }
                let m = if idx < mats.len() { &mats[idx] } else { mats.last().unwrap() };
                if gpu {
                    if !crate::gpu::apply_filter_gpu(&mut frame, ww, hh, m, cfg2.strength) {
                        apply_filter(&mut frame, ww, m, cfg2.strength);
                    }
                } else {
                    apply_filter(&mut frame, ww, m, cfg2.strength);
                }
                if tx.send((idx, frame)).is_err() { break; }
            }
        }));
    }
    drop(rx_raw);
    drop(tx_done);

    let cancel_dec = cancel.clone();
    let dec_handle = std::thread::spawn(move || {
        let mut idx = 0usize;
        loop {
            if cancel_dec.load(std::sync::atomic::Ordering::SeqCst) { let _ = dec.kill(); break; }
            let mut fbuf = vec![0u8; frame_bytes];
            if dec_out.read_exact(&mut fbuf).is_err() { break; }
            if tx_raw.send((idx, fbuf)).is_err() { break; }
            idx += 1;
        }
        let _ = dec.wait();
    });

    let mut written = 0usize;
    while let Ok((_idx, frame)) = rx_done.recv() {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = enc.kill();
            for h in worker_handles { let _ = h.join(); }
            let _ = dec_handle.join();
            return Ok(());
        }
        enc_in.write_all(&frame).context("write frame")?;
        written += 1;
        if written % 30 == 0 {
            if std::env::var("DG_TIMING").is_ok() { eprintln!("frame {} at {:?}", written, t_after_pass1.elapsed()); }
            if let Some(tx) = &progress {
                let pct = written as f64 / frame_count.max(1) as f64;
                let _ = tx.send(crate::tui::ProgressEvent::VideoProgress(name.to_string(), pct));
            }
        }
    }
    if std::env::var("DG_TIMING").is_ok() { eprintln!("pass2 loop done: {} frames, {:?}", written, t_after_pass1.elapsed()); }
    drop(enc_in);
    for h in worker_handles { let _ = h.join(); }
    let _ = dec_handle.join();
    let t_enc = std::time::Instant::now();
    let status = enc.wait().context("encoder wait")?;
    if std::env::var("DG_TIMING").is_ok() { eprintln!("enc wait: {:?}", t_enc.elapsed()); }
    if !status.success() {
        anyhow::bail!("ffmpeg encoder failed");
    }
    if let Some(trf) = &trf_path {
        let _ = std::fs::remove_file(trf);
    }
    Ok(())
}
fn mean_abs_diff(a: &[u8], b: &[u8]) -> f32 {
    let n = a.len().min(b.len()).max(1);
    let mut sum = 0.0f32;
    for i in 0..n {
        sum += (a[i] as i16 - b[i] as i16).unsigned_abs() as f32;
    }
    sum / n as f32
}

fn smooth_samples(samples: Vec<(usize, Matrix)>) -> Vec<(usize, Matrix)> {
    if samples.len() < 3 { return samples; }
    let mut out = Vec::with_capacity(samples.len());
    for i in 0..samples.len() {
        let lo = i.saturating_sub(1);
        let hi = (i + 1).min(samples.len() - 1);
        let mut m = [0.0f32; 20];
        for k in 0..20 {
            let mut sum = 0.0f32;
            for j in lo..=hi {
                sum += samples[j].1[k];
            }
            m[k] = sum / (hi - lo + 1) as f32;
        }
        out.push((samples[i].0, m));
    }
    out
}
