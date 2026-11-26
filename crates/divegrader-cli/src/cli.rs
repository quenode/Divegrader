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

    #[arg(long, default_value_t = 2.0)]
    pub sample_seconds: f32,

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

    #[arg(long, default_value_t = 0)]
    pub jobs: usize,

    #[arg(long, default_value_t = 1.0)]
    pub strength: f32,

    #[arg(long, default_value_t = 60.0)]
    pub min_avg_red: f32,

    #[arg(long, default_value_t = 120.0)]
    pub max_hue_shift: f32,

    #[arg(long, default_value_t = 1.2)]
    pub blue_magic: f32,

    #[arg(long, default_value_t = 2000.0)]
    pub threshold_ratio: f32,

    #[arg(long, default_value = "default")]
    pub grade_preset: String,

    #[arg(long, default_value = "auto")]
    pub water: String,

    #[arg(long, default_value = "auto")]
    pub white_balance: String,

    #[arg(long, default_value_t = 0.0)]
    pub dehaze: f32,

    #[arg(long, default_value = "interpolate")]
    pub keyframes: String,

    #[arg(long, default_value = "auto")]
    pub depth: String,

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
