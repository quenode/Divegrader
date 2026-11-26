pub mod color;
pub mod config;

pub use color::{
    apply_filter, get_filter_matrix, get_filter_matrix_from_rgb, interpolate_matrices, Matrix,
};
pub use config::GradeConfig;
