pub mod color;
pub mod config;

pub use color::{
    analyze, analyze_from_rgb, apply_filter, get_filter_matrix, get_filter_matrix_from_rgb,
    interpolate_matrices, post_process, GradeResult, Matrix,
};
pub use config::GradeConfig;
