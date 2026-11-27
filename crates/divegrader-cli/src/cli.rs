use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "divegrader",
    version,
    about = "Underwater photo/video color correction"
)]
pub struct Cli {
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,

    #[arg(short, long)]
    pub recursive: bool,

    #[arg(short, long)]
    pub output: Option<PathBuf>,

    #[arg(long, default_value = "graded")]
    pub suffix: String,

    #[arg(long)]
    pub overwrite: bool,

    #[arg(long, default_value_t = 95)]
    pub quality: u8,

    #[arg(long)]
    pub sample_seconds: Option<f32>,

    #[arg(long, default_value_t = 18)]
    pub crf: u8,

    #[arg(long, default_value = "medium")]
    pub preset: String,

    #[arg(long, default_value = "x265")]
    pub codec: String,

    #[arg(long, default_value = "auto")]
    pub hw: String,

    #[arg(long)]
    pub gpu: bool,

    #[arg(long)]
    pub no_audio: bool,

    #[arg(long)]
    pub stabilize: bool,

    #[arg(long)]
    pub preview: Option<f64>,

    #[arg(long)]
    pub seek: Option<f64>,

    #[arg(long, default_value_t = 0)]
    pub jobs: usize,

    #[arg(long)]
    pub strength: Option<f32>,

    #[arg(long)]
    pub min_avg_red: Option<f32>,

    #[arg(long)]
    pub max_hue_shift: Option<f32>,

    #[arg(long)]
    pub blue_magic: Option<f32>,

    #[arg(long)]
    pub threshold_ratio: Option<f32>,

    #[arg(long)]
    pub max_gain: Option<f32>,

    #[arg(long)]
    pub green_scale: Option<f32>,

    #[arg(long)]
    pub green_gain_cap: Option<f32>,

    #[arg(long)]
    pub water: Option<String>,

    #[arg(long)]
    pub white_balance: Option<String>,

    #[arg(long)]
    pub black_point: Option<String>,

    #[arg(long)]
    pub exposure: Option<String>,

    #[arg(long)]
    pub blue_hue: Option<String>,

    #[arg(long)]
    pub dehaze: Option<f32>,

    #[arg(long)]
    pub keyframes: Option<String>,

    #[arg(long)]
    pub depth: Option<String>,

    #[arg(long, default_value = "default")]
    pub profile: String,

    #[arg(long)]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub save_config: Option<PathBuf>,

    #[arg(short, long)]
    pub quiet: bool,

    #[arg(long)]
    pub tui: bool,

    #[arg(long)]
    pub silent: bool,

    #[arg(short, long)]
    pub verbose: bool,
}
