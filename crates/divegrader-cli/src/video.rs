use anyhow::{Context, Result};
use divegrader_core::{
    analyze_from_rgb, apply_filter, interpolate_matrices, post_process, GradeConfig, GradeResult,
    Matrix,
};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn is_video(p: &Path) -> bool {
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .as_deref(),
        Some("mp4") | Some("mkv") | Some("mov") | Some("avi") | Some("webm")
    )
}

struct Probe {
    width: u32,
    height: u32,
    fps: f64,
    duration: f64,
    has_audio: bool,
}

fn probe(input: &str) -> Result<Probe> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,duration",
            "-of",
            "json",
            input,
        ])
        .output()
        .context("ffprobe")?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).context("parse ffprobe")?;
    let s = &v["streams"][0];
    let w = s["width"].as_u64().context("width")? as u32;
    let h = s["height"].as_u64().context("height")? as u32;
    let fr = s["r_frame_rate"].as_str().unwrap_or("30/1");
    let fps = parse_rate(fr);
    let duration = s["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);
    // audio check
    let aout = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
            input,
        ])
        .output()
        .context("ffprobe audio")?;
    let has_audio = !aout.stdout.is_empty();
    Ok(Probe {
        width: w,
        height: h,
        fps,
        duration,
        has_audio,
    })
}

fn parse_rate(s: &str) -> f64 {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let a: f64 = parts[0].parse().unwrap_or(30.0);
        let b: f64 = parts[1].parse().unwrap_or(1.0);
        if b != 0.0 {
            a / b
        } else {
            a
        }
    } else {
        s.parse().unwrap_or(30.0)
    }
}

fn use_nvenc(hw: &str) -> bool {
    if hw == "cpu" {
        return false;
    }
    if hw == "nvenc" {
        return true;
    }
    // auto: check ffmpeg for nvenc
    Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("hevc_nvenc"))
        .unwrap_or(false)
}

fn decoder(input: &str, hw: &str, seek_s: &str, eff_s: &str) -> Result<std::process::Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error"]);
    if hw != "cpu" {
        cmd.args(["-hwaccel", "cuda"]);
    }
    cmd.args([
        "-ss", seek_s, "-i", input, "-t", eff_s, "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    cmd.spawn().context("spawn ffmpeg decoder")
}

fn spawn_stderr_logger(
    stderr: std::process::ChildStderr,
    progress: Option<std::sync::mpsc::Sender<crate::tui::ProgressEvent>>,
    verbose: bool,
) {
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(tx) = &progress {
                let _ = tx.send(crate::tui::ProgressEvent::Log(format!(
                    "⚠ warning: {}",
                    line
                )));
            } else if verbose {
                eprintln!("ffmpeg: {}", line);
            }
        }
    });
}

pub fn process_video(
    input: &Path,
    output: &Path,
    cfg: &GradeConfig,
    cli: &crate::cli::Cli,
    progress: Option<std::sync::mpsc::Sender<crate::tui::ProgressEvent>>,
    name: &str,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    let input_s = input.to_string_lossy().to_string();
    let t0 = std::time::Instant::now();
    let probe = probe(&input_s)?;
    let w = probe.width;
    let h = probe.height;
    let fps = probe.fps;

    let seek = cli.seek.unwrap_or(0.0).max(0.0);
    let preview = cli.preview;
    if let Some(p) = preview {
        if p <= 0.0 {
            anyhow::bail!("--preview must be greater than 0");
        }
    }
    if probe.duration > 0.0 && seek >= probe.duration {
        anyhow::bail!(
            "--seek {} is beyond video duration {:.1}s",
            seek,
            probe.duration
        );
    }
    let effective_duration = preview.unwrap_or(probe.duration);
    let seek_s = format!("{:.3}", seek);
    let eff_s = format!("{:.3}", effective_duration);

    // stabilization analysis pass (vidstabdetect)
    let trf_path = if cli.stabilize {
        let p = std::env::temp_dir().join(format!("divegrader_{}.trf", std::process::id()));
        let vf = format!(
            "scale=640:360,vidstabdetect=shakiness=5:accuracy=15:result={}",
            p.display()
        );
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-progress",
                "pipe:2",
                "-nostats",
                "-ss",
                &seek_s,
                "-i",
                &input_s,
                "-t",
                &eff_s,
                "-vf",
                &vf,
                "-f",
                "null",
                "-",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("vidstabdetect")?;
        let stderr = child.stderr.take().context("vidstabdetect stderr")?;
        let dur_ms = effective_duration * 1000.0;
        let verbose = cli.verbose;
        if let Some(tx) = &progress {
            let _ = tx.send(crate::tui::ProgressEvent::Phase("Stabilizing".into()));
        }
        let prog: Option<std::sync::mpsc::Sender<crate::tui::ProgressEvent>> = progress.clone();
        let name_owned = name.to_string();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if line.contains("vid.stab") || line.contains("stalled") {
                    if let Some(tx) = &prog {
                        let _ = tx.send(crate::tui::ProgressEvent::Log(format!(
                            "⚠ warning: {}",
                            line
                        )));
                    } else if verbose {
                        eprintln!("vidstab warning: {}", line);
                    }
                    continue;
                }
                if let Some(tx) = &prog {
                    if let Some(v) = line.strip_prefix("out_time_ms=") {
                        let ms: f64 = v.trim().parse().unwrap_or(0.0);
                        if dur_ms > 0.0 {
                            let pct = (ms / dur_ms).clamp(0.0, 1.0);
                            let _ = tx.send(crate::tui::ProgressEvent::VideoProgress(
                                name_owned.clone(),
                                pct,
                            ));
                        }
                    }
                }
            }
        });
        let status = child.wait().context("vidstabdetect wait")?;
        if !status.success() {
            anyhow::bail!("vidstabdetect failed");
        }
        Some(p)
    } else {
        None
    };

    // Pass 1 (sampling only; decode keyframes at low res for speed)
    let mut child = if cli.hw != "cpu" {
        Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-hwaccel",
                "cuda",
                "-skip_frame",
                "nokey",
                "-progress",
                "pipe:2",
                "-nostats",
                "-ss",
                &seek_s,
                "-i",
                &input_s,
                "-t",
                &eff_s,
                "-vf",
                "scale=256:256",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn ffmpeg pass1 decoder")?
    } else {
        Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-skip_frame",
                "nokey",
                "-progress",
                "pipe:2",
                "-nostats",
                "-ss",
                &seek_s,
                "-i",
                &input_s,
                "-t",
                &eff_s,
                "-vf",
                "scale=256:256",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn ffmpeg pass1 decoder")?
    };
    let mut stdout = child.stdout.take().context("decoder stdout")?;
    let stderr = child.stderr.take().context("decoder stderr")?;
    let progress_pct: std::sync::Arc<std::sync::Mutex<f64>> =
        std::sync::Arc::new(std::sync::Mutex::new(0.0));
    let prog_clone = progress_pct.clone();
    let dur_ms = effective_duration * 1000.0;
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if let Some(v) = line.strip_prefix("out_time_ms=") {
                let ms: f64 = v.trim().parse().unwrap_or(0.0);
                if dur_ms > 0.0 {
                    *prog_clone.lock().unwrap() = (ms / dur_ms).clamp(0.0, 1.0);
                }
            }
        }
    });
    let small_bytes = 256 * 256 * 3;
    let mut buf = vec![0u8; small_bytes];
    let mut samples: Vec<GradeResult> = Vec::new();
    let mut decoded = 0usize;
    let mut last_sampled: Option<Vec<u8>> = None;
    if let Some(tx) = &progress {
        let _ = tx.send(crate::tui::ProgressEvent::Phase("Analyzing".into()));
    }
    loop {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = child.kill();
            return Ok(());
        }
        if stdout.read_exact(&mut buf).is_err() {
            break;
        }
        let scene_change = match &last_sampled {
            Some(prev) => mean_abs_diff(prev, &buf) > 30.0,
            None => true,
        };
        if samples.is_empty() || scene_change || samples.len() < 13 && decoded.is_multiple_of(3) {
            let m = analyze_from_rgb(256, 256, &buf, cfg);
            samples.push(m);
            last_sampled = Some(buf.clone());
        }
        decoded += 1;
        if decoded.is_multiple_of(30) {
            if let Some(tx) = &progress {
                let pct = *progress_pct.lock().unwrap();
                let _ = tx.send(crate::tui::ProgressEvent::VideoProgress(
                    name.to_string(),
                    pct,
                ));
            }
        }
    }
    let _ = child.wait();
    if cancel.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    if decoded == 0 {
        anyhow::bail!("no frames decoded");
    }
    // assign uniform frame positions to samples
    let total_frames = if effective_duration > 0.0 {
        (effective_duration * fps) as usize
    } else {
        decoded * (fps * cfg.sample_seconds as f64).round().max(1.0) as usize
    }
    .max(1);
    let mut samples_idx: Vec<(usize, GradeResult)> = Vec::with_capacity(samples.len());
    let n = samples.len();
    for (i, m) in samples.into_iter().enumerate() {
        let pos = if n <= 1 {
            0
        } else {
            (i * (total_frames - 1)) / (n - 1)
        };
        samples_idx.push((pos, m));
    }

    let matrix_samples: Vec<(usize, Matrix)> =
        samples_idx.iter().map(|(i, r)| (*i, r.matrix)).collect();
    let matrix_samples = smooth_samples(matrix_samples);
    let matrices = if cfg.keyframes == "median" {
        let mut m = [0.0f32; 20];
        for k in 0..20 {
            let mut vals: Vec<f32> = matrix_samples.iter().map(|(_, mm)| mm[k]).collect();
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
            m[k] = if vals.is_empty() {
                0.0
            } else {
                vals[vals.len() / 2]
            };
        }
        vec![m; total_frames]
    } else {
        interpolate_matrices(&matrix_samples, total_frames)
    };
    // median post-processing parameters across samples
    let post = {
        let mut bp0: Vec<f32> = samples_idx.iter().map(|(_, r)| r.black_point[0]).collect();
        let mut bp1: Vec<f32> = samples_idx.iter().map(|(_, r)| r.black_point[1]).collect();
        let mut bp2: Vec<f32> = samples_idx.iter().map(|(_, r)| r.black_point[2]).collect();
        let mut ex: Vec<f32> = samples_idx.iter().map(|(_, r)| r.exposure).collect();
        let mut bh: Vec<f32> = samples_idx.iter().map(|(_, r)| r.blue_hue).collect();
        bp0.sort_by(|a, b| a.partial_cmp(b).unwrap());
        bp1.sort_by(|a, b| a.partial_cmp(b).unwrap());
        bp2.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ex.sort_by(|a, b| a.partial_cmp(b).unwrap());
        bh.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mid = |v: &Vec<f32>| if v.is_empty() { 0.0 } else { v[v.len() / 2] };
        GradeResult {
            matrix: *matrices.last().unwrap_or(&[0.0f32; 20]),
            black_point: [mid(&bp0), mid(&bp1), mid(&bp2)],
            exposure: mid(&ex),
            blue_hue: mid(&bh),
        }
    };
    if std::env::var("DG_TIMING").is_ok() {
        eprintln!(
            "pass1 done: {} frames, {} samples, {:?}",
            decoded,
            samples_idx.len(),
            t0.elapsed()
        );
    }
    let t_after_pass1 = std::time::Instant::now();

    // Pass 2
    let mut dec = decoder(&input_s, &cli.hw, &seek_s, &eff_s)?;
    if let Some(stderr) = dec.stderr.take() {
        spawn_stderr_logger(stderr, progress.clone(), cli.verbose);
    }
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
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgb24".into(),
        "-s".into(),
        format!("{}x{}", w, h),
        "-r".into(),
        format!("{:.6}", fps),
        "-i".into(),
        "-".into(),
        "-i".into(),
        input_s.clone(),
    ];
    if cli.stabilize {
        if let Some(trf) = &trf_path {
            enc_args.extend([
                "-filter_complex".into(),
                format!(
                    "[0:v]vidstabtransform=input={}:zoom=5:smoothing=15[vo]",
                    trf.display()
                ),
                "-map".into(),
                "[vo]".into(),
            ]);
        } else {
            enc_args.extend(["-map".into(), "0:v:0".into()]);
        }
    } else {
        enc_args.extend(["-map".into(), "0:v:0".into()]);
    }
    let preview_mode = preview.is_some();
    if !cli.no_audio && !preview_mode && probe.has_audio {
        enc_args.extend(["-map".into(), "1:a?".into()]);
    }
    if preview_mode {
        enc_args.extend(["-an".into()]);
    }
    enc_args.extend(["-c:v".into(), codec_ffmpeg.into()]);
    if nvenc {
        enc_args.extend([
            "-rc".into(),
            "vbr".into(),
            "-cq".into(),
            cli.crf.to_string(),
        ]);
    } else {
        enc_args.extend(["-crf".into(), cli.crf.to_string()]);
    }
    enc_args.extend(["-preset".into(), cli.preset.clone()]);
    if !cli.no_audio && !preview_mode && probe.has_audio {
        enc_args.extend(["-c:a".into(), "copy".into()]);
    }
    enc_args.extend([
        "-map_metadata".into(),
        "1".into(),
        "-movflags".into(),
        "+faststart".into(),
        output.to_string_lossy().to_string(),
    ]);

    let mut enc = Command::new("ffmpeg")
        .args(&enc_args)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn ffmpeg encoder")?;
    if let Some(stderr) = enc.stderr.take() {
        spawn_stderr_logger(stderr, progress.clone(), cli.verbose);
    }
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
        let post2 = post.clone();
        let cfg2 = cfg.clone();
        let gpu = cli.gpu;
        let cancel2 = cancel.clone();
        worker_handles.push(std::thread::spawn(move || {
            while let Ok(item) = rx.recv() {
                let (idx, mut frame): (usize, Vec<u8>) = item;
                if cancel2.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let m = if idx < mats.len() {
                    &mats[idx]
                } else {
                    mats.last().unwrap()
                };
                if gpu {
                    if !crate::gpu::apply_filter_gpu(
                        &mut frame,
                        ww,
                        hh,
                        m,
                        cfg2.strength,
                        cfg2.dehaze,
                    ) {
                        apply_filter(&mut frame, ww, m, cfg2.strength, cfg2.dehaze);
                    }
                } else {
                    apply_filter(&mut frame, ww, m, cfg2.strength, cfg2.dehaze);
                }
                post_process(&mut frame, ww, &post2, &cfg2);
                if tx.send((idx, frame)).is_err() {
                    break;
                }
            }
        }));
    }
    drop(rx_raw);
    drop(tx_done);

    let cancel_dec = cancel.clone();
    let dec_handle = std::thread::spawn(move || {
        let mut idx = 0usize;
        loop {
            if cancel_dec.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = dec.kill();
                break;
            }
            let mut fbuf = vec![0u8; frame_bytes];
            if dec_out.read_exact(&mut fbuf).is_err() {
                break;
            }
            if tx_raw.send((idx, fbuf)).is_err() {
                break;
            }
            idx += 1;
        }
        let _ = dec.wait();
    });

    let mut written = 0usize;
    if let Some(tx) = &progress {
        let _ = tx.send(crate::tui::ProgressEvent::Phase("Grading".into()));
    }
    while let Ok((_idx, frame)) = rx_done.recv() {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = enc.kill();
            for h in worker_handles {
                let _ = h.join();
            }
            let _ = dec_handle.join();
            return Ok(());
        }
        enc_in.write_all(&frame).context("write frame")?;
        written += 1;
        if written.is_multiple_of(30) {
            if std::env::var("DG_TIMING").is_ok() {
                eprintln!("frame {} at {:?}", written, t_after_pass1.elapsed());
            }
            if let Some(tx) = &progress {
                let pct = written as f64 / total_frames.max(1) as f64;
                let _ = tx.send(crate::tui::ProgressEvent::VideoProgress(
                    name.to_string(),
                    pct,
                ));
            }
        }
    }
    if std::env::var("DG_TIMING").is_ok() {
        eprintln!(
            "pass2 loop done: {} frames, {:?}",
            written,
            t_after_pass1.elapsed()
        );
    }
    if let Some(tx) = &progress {
        let _ = tx.send(crate::tui::ProgressEvent::Phase("Finishing".into()));
    }
    drop(enc_in);
    for h in worker_handles {
        let _ = h.join();
    }
    let _ = dec_handle.join();
    let t_enc = std::time::Instant::now();
    let status = enc.wait().context("encoder wait")?;
    if std::env::var("DG_TIMING").is_ok() {
        eprintln!("enc wait: {:?}", t_enc.elapsed());
    }
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
    if samples.len() < 3 {
        return samples;
    }
    let mut out = Vec::with_capacity(samples.len());
    for i in 0..samples.len() {
        let lo = i.saturating_sub(1);
        let hi = (i + 1).min(samples.len() - 1);
        let mut m = [0.0f32; 20];
        for k in 0..20 {
            let mut sum = 0.0f32;
            for sample in &samples[lo..=hi] {
                sum += sample.1[k];
            }
            m[k] = sum / (hi - lo + 1) as f32;
        }
        out.push((samples[i].0, m));
    }
    out
}
