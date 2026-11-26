use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Terminal,
};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default, Clone)]
pub struct SystemStats {
    pub cpu_usage: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    pub gpu_name: String,
    pub gpu_util: u32,
    pub gpu_mem_used: u64,
    pub gpu_mem_total: u64,
    pub gpu_temp: u32,
    pub gpu_driver: String,
    pub os_name: String,
    pub hostname: String,
    pub ffmpeg_version: String,
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum ProgressEvent {
    FileStart(String),
    FileDone(String, bool),
    VideoProgress(String, f64),
}

pub fn spawn_stats(stats: Arc<Mutex<SystemStats>>) {
    std::thread::spawn(move || {
        #[cfg(feature = "nvidia")]
        let nvml = nvml_wrapper::Nvml::init().ok();
        #[cfg(feature = "nvidia")]
        let device = nvml.as_ref().and_then(|n| n.device_by_index(0).ok());

        let mut sys = sysinfo::System::new_all();
        let os_name = sysinfo::System::name().unwrap_or_default();
        let hostname = sysinfo::System::host_name().unwrap_or_default();
        let ffmpeg_version = Command::new("ffmpeg")
            .arg("-version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.lines().next().map(|l| l.to_string()))
            .unwrap_or_else(|| "unknown".to_string());

        loop {
            sys.refresh_all();
            let cpu = sys.global_cpu_usage();
            let ram_used = sys.used_memory();
            let ram_total = sys.total_memory();

            #[cfg(feature = "nvidia")]
            let (gpu_name, gpu_util, gpu_mem_used, gpu_mem_total, gpu_temp, gpu_driver) =
                if let Some(dev) = &device {
                    let name = dev.name().unwrap_or_default();
                    let util = dev.utilization_rates().map(|u| u.gpu).unwrap_or(0);
                    let mem = dev.memory_info().ok();
                    let (used, total) = mem.map(|m| (m.used, m.total)).unwrap_or((0, 0));
                    let temp = dev
                        .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                        .unwrap_or(0);
                    let drv = nvml
                        .as_ref()
                        .and_then(|n| n.sys_driver_version().ok())
                        .unwrap_or_default();
                    (name, util, used, total, temp, drv)
                } else {
                    (String::from("N/A"), 0, 0, 0, 0, String::from("N/A"))
                };

            #[cfg(not(feature = "nvidia"))]
            let (gpu_name, gpu_util, gpu_mem_used, gpu_mem_total, gpu_temp, gpu_driver) = (
                String::from("N/A"),
                0,
                0u64,
                0u64,
                0u32,
                String::from("N/A"),
            );

            let mut s = stats.lock().unwrap();
            s.cpu_usage = cpu;
            s.ram_used = ram_used;
            s.ram_total = ram_total;
            s.gpu_name = gpu_name;
            s.gpu_util = gpu_util;
            s.gpu_mem_used = gpu_mem_used;
            s.gpu_mem_total = gpu_mem_total;
            s.gpu_temp = gpu_temp;
            s.gpu_driver = gpu_driver;
            s.os_name = os_name.clone();
            s.hostname = hostname.clone();
            s.ffmpeg_version = ffmpeg_version.clone();
            drop(s);
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

fn grad_color(ratio: f64, frame: u64) -> Color {
    // Catppuccin Mocha: red -> yellow -> green
    let (r, g, b) = if ratio < 0.5 {
        let t = ratio * 2.0;
        (
            0xf3u8,
            (0x8b as f64 + t * (0xe2 as f64 - 0x8b as f64)) as u8,
            (0xa8 as f64 + t * (0xaf as f64 - 0xa8 as f64)) as u8,
        )
    } else {
        let t = (ratio - 0.5) * 2.0;
        (
            (0xf3 as f64 + t * (0xa6 as f64 - 0xf3 as f64)) as u8,
            (0xe2 as f64 + t * (0xe3 as f64 - 0xe2 as f64)) as u8,
            (0xaf as f64 + t * (0xa1 as f64 - 0xaf as f64)) as u8,
        )
    };
    let shimmer = ((frame as f64 * 0.3).sin() * 12.0) as i16;
    let g = (g as i16 + shimmer).clamp(0, 255) as u8;
    Color::Rgb(r, g, b)
}

pub fn run_tui(
    rx: std::sync::mpsc::Receiver<ProgressEvent>,
    stats: Arc<Mutex<SystemStats>>,
    total: usize,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) {
    enable_raw_mode().unwrap();
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut done = 0usize;
    let mut current = String::new();
    let mut current_pct = 0.0f64;
    let mut logs: Vec<String> = Vec::new();
    let mut frame: u64 = 0;
    let mut cpu_history: Vec<f32> = Vec::new();
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let start = Instant::now();

    loop {
        while let Ok(ev) = rx.try_recv() {
            match ev {
                ProgressEvent::FileStart(n) => {
                    current = n;
                    current_pct = 0.0;
                }
                ProgressEvent::FileDone(n, ok) => {
                    done += 1;
                    let line = if ok {
                        format!("✓ {}", n)
                    } else {
                        format!("✗ {}", n)
                    };
                    logs.push(line);
                    if logs.len() > 12 {
                        logs.remove(0);
                    }
                }
                ProgressEvent::VideoProgress(_, p) => current_pct = p,
            }
        }

        let s = stats.lock().unwrap().clone();
        cpu_history.push(s.cpu_usage);
        if cpu_history.len() > 10 {
            cpu_history.remove(0);
        }
        let frame_now = frame;
        terminal
            .draw(|f| {
                let size = f.area();
                // outer horizontal split
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
                    .split(size);

                // left column
                let left = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Min(4),
                    ])
                    .split(cols[0]);

                // right column
                let right = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(7),
                        Constraint::Length(9),
                        Constraint::Length(6),
                        Constraint::Min(3),
                    ])
                    .split(cols[1]);
                // upper-right split: cpu/memory | cpu fire
                let cpu_row = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .split(right[0]);
                // gpu row split: graphics | gpu load
                let gpu_row = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .split(right[1]);

                // shadow + title
                let spin = spinner[(frame_now as usize) % spinner.len()];
                let title = Paragraph::new(format!(
                    "{} divegrader  {} / {} files  {:.1}s",
                    spin,
                    done,
                    total,
                    start.elapsed().as_secs_f64()
                ))
                .style(Style::default().fg(Color::Rgb(0x89, 0xb4, 0xfa)))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Rgb(0xcb, 0xa6, 0xf7))),
                );
                f.render_widget(title, left[0]);

                // overall gauge
                let overall_ratio = if total > 0 {
                    ((done as f64 + current_pct) / total as f64).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let overall = Gauge::default()
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!(
                                "Overall  {} / {}  {:.1}s",
                                done,
                                total,
                                start.elapsed().as_secs_f64()
                            ))
                            .border_style(Style::default().fg(Color::Rgb(0x89, 0xb4, 0xfa))),
                    )
                    .gauge_style(Style::default().fg(grad_color(overall_ratio, frame_now)))
                    .ratio(overall_ratio);
                f.render_widget(overall, left[1]);

                // current gauge
                let cur = Gauge::default()
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("Current: {}", current))
                            .border_style(Style::default().fg(Color::Rgb(0xf9, 0xe2, 0xaf))),
                    )
                    .gauge_style(Style::default().fg(grad_color(current_pct, frame_now)))
                    .ratio(current_pct.clamp(0.0, 1.0));
                f.render_widget(cur, left[2]);

                // logs
                let log_text = logs.join("\n");
                let logw = Paragraph::new(log_text).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Log")
                        .border_style(Style::default().fg(Color::Rgb(0xf3, 0x8b, 0xa8))),
                );
                f.render_widget(logw, left[3]);

                // right: cpu/ram
                let cpu_text = format!(
                    "CPU: {:.1}%\nRAM: {:.1} / {:.1} GB",
                    s.cpu_usage,
                    s.ram_used as f64 / 1e9,
                    s.ram_total as f64 / 1e9
                );
                let cpuw = Paragraph::new(cpu_text).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("CPU / Memory")
                        .border_style(Style::default().fg(Color::Rgb(0x89, 0xb4, 0xfa))),
                );
                f.render_widget(cpuw, cpu_row[0]);

                // gpu
                let gpu_text = format!(
                    "GPU: {}\nDriver: {}\nUtil: {}%\nVRAM: {:.1} / {:.1} GB\nTemp: {}°C",
                    s.gpu_name,
                    s.gpu_driver,
                    s.gpu_util,
                    s.gpu_mem_used as f64 / 1e9,
                    s.gpu_mem_total as f64 / 1e9,
                    s.gpu_temp
                );
                let gpuw = Paragraph::new(gpu_text).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Graphics")
                        .border_style(Style::default().fg(Color::Rgb(0xa6, 0xe3, 0xa1))),
                );
                f.render_widget(gpuw, gpu_row[0]);

                // software
                let sw_text = format!(
                    "OS: {}\nHost: {}\nffmpeg: {}\ndivegrader: {}",
                    s.os_name,
                    s.hostname,
                    s.ffmpeg_version,
                    env!("CARGO_PKG_VERSION")
                );
                let sww = Paragraph::new(sw_text).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Software")
                        .border_style(Style::default().fg(Color::Rgb(0xf9, 0xe2, 0xaf))),
                );
                f.render_widget(sww, right[2]);

                // vertical cpu load bar
                let levels = 6usize;
                let filled = ((s.cpu_usage / 100.0) * levels as f32).round() as usize;
                let mut fire_lines: Vec<Line> = Vec::new();
                for level in (0..levels).rev() {
                    let ch = if level < filled {
                        "███"
                    } else {
                        "░░░"
                    };
                    let color = match level {
                        5 => Color::Rgb(0xf3, 0x8b, 0xa8),
                        4 => Color::Rgb(0xf9, 0xe2, 0xaf),
                        3 => Color::Rgb(0xf9, 0xe2, 0xaf),
                        2 => Color::Rgb(0xa6, 0xe3, 0xa1),
                        1 => Color::Rgb(0xa6, 0xe3, 0xa1),
                        _ => Color::Rgb(0x89, 0xb4, 0xfa),
                    };
                    fire_lines.push(Line::from(Span::styled(ch, Style::default().fg(color))));
                }
                fire_lines.push(Line::from(format!("{:5.1}%", s.cpu_usage)));
                let fire = Paragraph::new(fire_lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("CPU load")
                        .border_style(Style::default().fg(Color::Rgb(0xf3, 0x8b, 0xa8))),
                );
                f.render_widget(fire, cpu_row[1]);
                // gpu load vertical bar
                let gpu_filled = ((s.gpu_util as f32 / 100.0) * 6.0).round() as usize;
                let mut gpu_lines: Vec<Line> = Vec::new();
                for level in (0..6).rev() {
                    let ch = if level < gpu_filled {
                        "███"
                    } else {
                        "░░░"
                    };
                    let color = match level {
                        5 => Color::Rgb(0xf3, 0x8b, 0xa8),
                        4 => Color::Rgb(0xf9, 0xe2, 0xaf),
                        3 => Color::Rgb(0xf9, 0xe2, 0xaf),
                        2 => Color::Rgb(0xa6, 0xe3, 0xa1),
                        1 => Color::Rgb(0xa6, 0xe3, 0xa1),
                        _ => Color::Rgb(0x89, 0xb4, 0xfa),
                    };
                    gpu_lines.push(Line::from(Span::styled(ch, Style::default().fg(color))));
                }
                gpu_lines.push(Line::from(format!("{:3}%", s.gpu_util)));
                let gpu_fire = Paragraph::new(gpu_lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("GPU load")
                        .border_style(Style::default().fg(Color::Rgb(0xa6, 0xe3, 0xa1))),
                );
                f.render_widget(gpu_fire, gpu_row[1]);
            })
            .unwrap();

        if event::poll(Duration::from_millis(100)).unwrap() {
            if let Event::Key(k) = event::read().unwrap() {
                if k.code == KeyCode::Char('q') {
                    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    break;
                }
            }
        }

        frame = frame.wrapping_add(1);
        if total > 0 && done >= total {
            std::thread::sleep(Duration::from_millis(1000));
            break;
        }
    }

    disable_raw_mode().unwrap();
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show).unwrap();
    terminal.show_cursor().unwrap();
    use std::io::Write;
    let _ = terminal.backend_mut().flush();
}

use std::process::Command;
