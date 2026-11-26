use std::path::{Path, PathBuf};

pub fn collect_inputs(inputs: &[PathBuf], recursive: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in inputs {
        if p.is_dir() {
            if recursive {
                for entry in walkdir(p) {
                    if entry.is_file() {
                        out.push(entry);
                    }
                }
            } else {
                if let Ok(rd) = std::fs::read_dir(p) {
                    for e in rd.flatten() {
                        let path = e.path();
                        if path.is_file() {
                            out.push(path);
                        }
                    }
                }
            }
        } else if p.is_file() {
            out.push(p.clone());
        }
    }
    out
}

fn walkdir(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                out.extend(walkdir(&path));
            } else {
                out.push(path);
            }
        }
    }
    out
}

pub fn output_path(input: &Path, output: Option<&Path>, suffix: &str) -> PathBuf {
    if let Some(o) = output {
        if o.is_dir() || !o.exists() && o.extension().is_none() {
            // treat as dir; preserve relative structure from cwd
            let name = input.file_stem().unwrap_or_default().to_string_lossy();
            let ext = input
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            let cwd = std::env::current_dir().unwrap_or_default();
            let rel = input.strip_prefix(&cwd).unwrap_or(input);
            let rel_parent = rel.parent().unwrap_or_else(|| Path::new(""));
            let dir = o.join(rel_parent);
            let _ = std::fs::create_dir_all(&dir);
            return dir.join(format!("{}_{}.{}", name, suffix, ext));
        }
        return o.to_path_buf();
    }
    let name = input.file_stem().unwrap_or_default().to_string_lossy();
    let ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{}_{}.{}", name, suffix, ext))
}

pub fn is_media(p: &Path) -> bool {
    crate::image_proc::is_image(p) || crate::video::is_video(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn output_path_file_input() {
        let p = output_path(Path::new("foo.jpg"), None, "graded");
        assert_eq!(p, Path::new("foo_graded.jpg"));
    }

    #[test]
    fn collect_inputs_file() {
        let tmp = std::env::temp_dir().join("dg_test.txt");
        std::fs::write(&tmp, "x").unwrap();
        let out = collect_inputs(std::slice::from_ref(&tmp), false);
        assert_eq!(out.len(), 1);
        let _ = std::fs::remove_file(&tmp);
    }
}
