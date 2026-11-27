mod cli;
mod gpu;
mod image_proc;
mod tui;
mod util;
mod video;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use divegrader_core::GradeConfig;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {:#}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let mut cfg = if let Some(cfg_path) = &cli.config {
        let s = std::fs::read_to_string(cfg_path)?;
        GradeConfig::from_toml(&s)?
    } else {
        GradeConfig::default()
    };

    // profile (scene presets), then explicit flags override
    cfg.profile = cli.profile.clone();
    cfg.apply_profile();

    if let Some(v) = cli.min_avg_red {
        cfg.min_avg_red = v;
    }
    if let Some(v) = cli.max_hue_shift {
        cfg.max_hue_shift = v;
    }
    if let Some(v) = cli.blue_magic {
        cfg.blue_magic = v;
    }
    if let Some(v) = cli.sample_seconds {
        cfg.sample_seconds = v;
    }
    if let Some(v) = cli.threshold_ratio {
        cfg.threshold_ratio = v;
    }
    if let Some(v) = cli.strength {
        cfg.strength = v;
    }
    if let Some(v) = cli.water.clone() {
        cfg.water = v;
    }
    if let Some(v) = cli.depth.clone() {
        cfg.depth = v;
    }
    if let Some(v) = cli.dehaze {
        cfg.dehaze = v;
    }
    if let Some(v) = cli.max_gain {
        cfg.max_gain = v;
    }
    if let Some(v) = cli.green_scale {
        cfg.green_scale = v;
    }
    if let Some(v) = cli.green_gain_cap {
        cfg.green_gain_cap = v;
    }
    if let Some(v) = cli.white_balance.clone() {
        cfg.white_balance = v;
    }
    if let Some(v) = cli.black_point.clone() {
        cfg.black_point = v;
    }
    if let Some(v) = cli.exposure.clone() {
        cfg.exposure = v;
    }
    if let Some(v) = cli.blue_hue.clone() {
        cfg.blue_hue = v;
    }
    if let Some(v) = cli.keyframes.clone() {
        cfg.keyframes = v;
    }

    if let Some(save) = &cli.save_config {
        std::fs::write(save, cfg.to_toml()?)?;
    }

    let inputs = util::collect_inputs(&cli.inputs, cli.recursive);
    let media: Vec<PathBuf> = inputs.into_iter().filter(|p| util::is_media(p)).collect();

    if media.is_empty() {
        println!("No media files found.");
        return Ok(());
    }

    let use_tui = !cli.silent && !cli.quiet && (cli.tui || atty_stdout());
    let (tx, rx) = mpsc::channel();
    let stats = Arc::new(Mutex::new(tui::SystemStats::default()));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let tui_handle = if use_tui {
        tui::spawn_stats(stats.clone());
        let rx_clone = rx;
        let stats_clone = stats.clone();
        let total = media.len();
        let cancel_clone = cancel.clone();
        let cfg_tui = cfg.clone();
        let actions = if cli.stabilize { 4 } else { 3 };
        Some(std::thread::spawn(move || {
            tui::run_tui(rx_clone, stats_clone, total, cancel_clone, cfg_tui, actions);
        }))
    } else {
        None
    };

    let jobs = if cli.jobs == 0 { num_cpus() } else { cli.jobs };
    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build_global()
        .ok();

    let errors = std::sync::atomic::AtomicUsize::new(0);
    let error_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let quiet = cli.quiet || cli.silent;
    media.par_iter().for_each(|input| {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let out = util::output_path(input, cli.output.as_deref(), &cli.suffix);
        let name = input
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let _ = tx.send(tui::ProgressEvent::FileStart(name.clone()));
        if cli.verbose {
            eprintln!(
                "{}: profile={} min_avg_red={} blue_magic={} strength={} dehaze={} water={} depth={} max_gain={} green_gain_cap={} white_balance={} black_point={} exposure={} blue_hue={}",
                input.display(),
                cfg.profile,
                cfg.min_avg_red,
                cfg.blue_magic,
                cfg.strength,
                cfg.dehaze,
                cfg.water,
                cfg.depth,
                cfg.max_gain,
                cfg.green_gain_cap,
                cfg.white_balance,
                cfg.black_point,
                cfg.exposure,
                cfg.blue_hue,
            );
        }
        let res = if image_proc::is_image(input) {
            image_proc::process_image(input, &out, &cfg, cli.quality, cli.overwrite, cli.gpu)
        } else {
            video::process_video(
                input,
                &out,
                &cfg,
                &cli,
                Some(tx.clone()),
                &name,
                cancel.clone(),
            )
        };
        match res {
            Ok(_) => {
                let _ = tx.send(tui::ProgressEvent::FileDone(name.clone(), true));
            }
            Err(e) => {
                errors.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                error_log
                    .lock()
                    .unwrap()
                    .push(format!("{}: {:#}", input.display(), e));
                let _ = tx.send(tui::ProgressEvent::FileDone(name.clone(), false));
                if !quiet {
                    eprintln!("{}: {:#}", input.display(), e);
                }
            }
        }
        if !use_tui && !quiet {
            // indicatif not used; simple print
            println!("done: {}", input.display());
        }
    });

    if let Some(h) = tui_handle {
        let _ = h.join();
    }

    let e = errors.load(std::sync::atomic::Ordering::SeqCst);
    if !quiet {
        if e > 0 {
            let log_path = match &cli.output {
                Some(o) if o.is_dir() || o.extension().is_none() => o.join("divegrader-errors.log"),
                Some(_) => std::path::PathBuf::from("divegrader-errors.log"),
                None => std::path::PathBuf::from("divegrader-errors.log"),
            };
            let lines = error_log.lock().unwrap();
            let mut content = format!("divegrader finished with {} error(s)\n\n", e);
            for l in lines.iter() {
                content.push_str(l);
                content.push('\n');
            }
            if let Err(we) = std::fs::write(&log_path, content) {
                eprintln!("could not write error log {}: {:#}", log_path.display(), we);
            } else {
                println!(
                    "Finished with {} errors. Error log: {}",
                    e,
                    log_path.display()
                );
            }
        } else {
            println!("All done.");
        }
    }
    Ok(())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

fn atty_stdout() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
