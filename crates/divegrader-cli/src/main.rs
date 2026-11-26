mod cli;
mod gpu;
mod image_proc;
mod tui;
mod util;
mod video;

use anyhow::Result;
use cli::Cli;
use clap::Parser;
use divegrader_core::GradeConfig;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use rayon::prelude::*;
use std::sync::mpsc;

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
        let mut c = GradeConfig::from_toml(&s)?;
        c.profile = cli.profile.clone();
        c.apply_profile();
        c
    } else {
        let mut c = GradeConfig::default();
        c.min_avg_red = cli.min_avg_red;
        c.max_hue_shift = cli.max_hue_shift;
        c.blue_magic = cli.blue_magic;
        c.sample_seconds = cli.sample_seconds;
        c.threshold_ratio = cli.threshold_ratio;
        c.strength = cli.strength;
        c.preset = cli.grade_preset.clone();
        c.water = cli.water.clone();
        c.depth = cli.depth.clone();
        c.profile = cli.profile.clone();
        c.apply_preset();
        c.apply_profile();
        c
    };

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
        Some(std::thread::spawn(move || {
            tui::run_tui(rx_clone, stats_clone, total, cancel_clone);
        }))
    } else {
        None
    };

    let jobs = if cli.jobs == 0 { num_cpus() } else { cli.jobs };
    rayon::ThreadPoolBuilder::new().num_threads(jobs).build_global().ok();

    let errors = std::sync::atomic::AtomicUsize::new(0);
    let quiet = cli.quiet || cli.silent;
    media.par_iter().for_each(|input| {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let out = util::output_path(input, cli.output.as_deref(), &cli.suffix);
        let name = input.file_name().unwrap_or_default().to_string_lossy().to_string();
        let _ = tx.send(tui::ProgressEvent::FileStart(name.clone()));
        let res = if image_proc::is_image(input) {
            image_proc::process_image(input, &out, &cfg, cli.quality, cli.overwrite, cli.gpu)
        } else {
            video::process_video(input, &out, &cfg, &cli, Some(tx.clone()), &name, cancel.clone())
        };
        match res {
            Ok(_) => { let _ = tx.send(tui::ProgressEvent::FileDone(name.clone(), true)); }
            Err(e) => {
                errors.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let _ = tx.send(tui::ProgressEvent::FileDone(name.clone(), false));
                if !quiet { eprintln!("{}: {:#}", input.display(), e); }
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
        if e > 0 { println!("Completed with {} errors.", e); }
        else { println!("All done."); }
    }
    Ok(())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

fn atty_stdout() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}