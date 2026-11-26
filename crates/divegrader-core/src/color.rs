use crate::config::GradeConfig;
use image::{Rgb, RgbImage};
use rayon::prelude::*;

pub type Matrix = [f32; 20];

#[inline]
fn hue_shift_pixel(r: f32, g: f32, b: f32, h: f32) -> (f32, f32, f32) {
    let u = (h * std::f32::consts::PI / 180.0).cos();
    let w = (h * std::f32::consts::PI / 180.0).sin();
    let nr = (0.299 + 0.701 * u + 0.168 * w) * r;
    let ng = (0.587 - 0.587 * u + 0.330 * w) * g;
    let nb = (0.114 - 0.114 * u - 0.497 * w) * b;
    (nr, ng, nb)
}

pub fn get_filter_matrix(img: &RgbImage, cfg: &GradeConfig) -> Matrix {
    let small = image::imageops::resize(img, 256, 256, image::imageops::FilterType::Nearest);
    let (w, h) = small.dimensions();

    // grey-world white balance pre-pass
    let work = if cfg.white_balance != "off" {
        let mut mr = 0.0f32;
        let mut mg = 0.0f32;
        let mut mb = 0.0f32;
        for p in small.pixels() {
            mr += p[0] as f32;
            mg += p[1] as f32;
            mb += p[2] as f32;
        }
        let n = (w * h) as f32;
        let (mr, mg, mb) = (mr / n, mg / n, mb / n);
        let avg = (mr + mg + mb) / 3.0;
        let apply = if cfg.white_balance == "auto" {
            mr.max(mg).max(mb) > 1.3 * mr.min(mg).min(mb).max(1.0)
        } else {
            true
        };
        if apply {
            let mut out = RgbImage::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    let p = small.get_pixel(x, y);
                    let r = (p[0] as f32 * (avg / mr.max(1.0))).clamp(0.0, 255.0) as u8;
                    let g = (p[1] as f32 * (avg / mg.max(1.0))).clamp(0.0, 255.0) as u8;
                    let b = (p[2] as f32 * (avg / mb.max(1.0))).clamp(0.0, 255.0) as u8;
                    out.put_pixel(x, y, Rgb([r, g, b]));
                }
            }
            out
        } else {
            small
        }
    } else {
        small
    };
    let (w, h) = work.dimensions();

    // average rgb
    let mut sum_r = 0.0f32;
    let mut sum_g = 0.0f32;
    let mut sum_b = 0.0f32;
    let n = (w * h) as f32;
    for p in work.pixels() {
        sum_r += p[0] as f32;
        sum_g += p[1] as f32;
        sum_b += p[2] as f32;
    }
    let avg = [sum_r / n, sum_g / n, sum_b / n];

    // depth / water aware auto-tuning
    let maxgb = avg[1].max(avg[2]).max(1.0);
    let deficit = 1.0 - avg[0] / maxgb;
    let water_green = avg[1] >= avg[2];
    let min_avg_red = if cfg.depth == "auto" {
        if deficit > 0.6 {
            75.0
        } else if deficit > 0.35 {
            65.0
        } else if deficit > 0.15 {
            55.0
        } else {
            50.0
        }
    } else {
        cfg.min_avg_red
    };
    let blue_magic = if cfg.water == "auto" {
        if water_green {
            1.05
        } else {
            1.25
        }
    } else {
        cfg.blue_magic
    };

    // find hue shift
    let mut hue_shift = 0.0f32;
    loop {
        let (rr, gg, bb) = hue_shift_pixel(avg[0], avg[1], avg[2], hue_shift);
        let new_avg_r = rr + gg + bb;
        if new_avg_r >= min_avg_red || hue_shift >= cfg.max_hue_shift {
            break;
        }
        hue_shift += 1.0;
    }

    // apply hue shift to whole image, replace red channel with sum of shifted channels
    let mut red_replaced: Vec<f32> = Vec::with_capacity((w * h) as usize);
    let mut g_vals: Vec<f32> = Vec::with_capacity((w * h) as usize);
    let mut b_vals: Vec<f32> = Vec::with_capacity((w * h) as usize);
    for p in work.pixels() {
        let (r, g, b) = hue_shift_pixel(p[0] as f32, p[1] as f32, p[2] as f32, hue_shift);
        let new_r = r + g + b;
        red_replaced.push(new_r.clamp(0.0, 255.0));
        g_vals.push(p[1] as f32);
        b_vals.push(p[2] as f32);
    }

    // histograms
    let mut hist_r = [0u32; 256];
    let mut hist_g = [0u32; 256];
    let mut hist_b = [0u32; 256];
    for &v in &red_replaced {
        hist_r[v as usize] += 1;
    }
    for &v in &g_vals {
        hist_g[v as usize] += 1;
    }
    for &v in &b_vals {
        hist_b[v as usize] += 1;
    }

    let threshold_level = (w * h) as f32 / cfg.threshold_ratio;

    let mut norm_r = vec![0.0f32; 256];
    let mut norm_g = vec![0.0f32; 256];
    let mut norm_b = vec![0.0f32; 256];
    for x in 0..256usize {
        if hist_r[x] as f32 <= threshold_level {
            norm_r[x] = x as f32;
        }
        if hist_g[x] as f32 <= threshold_level {
            norm_g[x] = x as f32;
        }
        if hist_b[x] as f32 <= threshold_level {
            norm_b[x] = x as f32;
        }
    }
    norm_r[255] = 255.0;
    norm_g[255] = 255.0;
    norm_b[255] = 255.0;

    fn normalizing_interval(arr: &[f32]) -> (f32, f32) {
        let mut high = 255.0f32;
        let mut low = 0.0f32;
        let mut max_dist = 0.0f32;
        for i in 1..arr.len() {
            let dist = arr[i] - arr[i - 1];
            if dist > max_dist {
                max_dist = dist;
                high = arr[i];
                low = arr[i - 1];
            }
        }
        (low, high)
    }

    let (r_low, r_high) = normalizing_interval(&norm_r);
    let (g_low, g_high) = normalizing_interval(&norm_g);
    let (b_low, b_high) = normalizing_interval(&norm_b);

    let shifted = hue_shift_pixel(1.0, 1.0, 1.0, hue_shift);
    let (sr, sg, sb) = shifted;

    let red_gain = 256.0 / (r_high - r_low).max(1.0);
    let green_gain = 256.0 / (g_high - g_low).max(1.0);
    let blue_gain = 256.0 / (b_high - b_low).max(1.0);

    let red_offset = (-r_low / 256.0) * red_gain;
    let green_offset = (-g_low / 256.0) * green_gain;
    let blue_offset = (-b_low / 256.0) * blue_gain;

    let adjust_red = sr * red_gain;
    let adjust_red_green = sg * red_gain;
    let adjust_red_blue = sb * red_gain * blue_magic;

    let mut m = [0.0f32; 20];
    m[0] = adjust_red;
    m[1] = adjust_red_green;
    m[2] = adjust_red_blue;
    m[4] = red_offset;
    m[6] = green_gain;
    m[9] = green_offset;
    m[12] = blue_gain;
    m[14] = blue_offset;
    m[18] = 1.0;
    m
}

pub fn apply_filter(pixels: &mut [u8], width: usize, matrix: &Matrix, strength: f32, dehaze: f32) {
    let row = width * 3;
    if row == 0 {
        return;
    }
    let orig: Vec<u8> = if strength < 1.0 {
        pixels.to_vec()
    } else {
        Vec::new()
    };
    let (m0, m1, m2, m4, m6, m9, m12, m14) = (
        matrix[0],
        matrix[1],
        matrix[2],
        matrix[4] * 255.0,
        matrix[6],
        matrix[9] * 255.0,
        matrix[12],
        matrix[14] * 255.0,
    );
    pixels
        .par_chunks_mut(row)
        .enumerate()
        .for_each(|(row_i, chunk)| {
            let len = chunk.len();
            let mut i = 0usize;
            while i + 2 < len {
                let r = chunk[i] as f32;
                let g = chunk[i + 1] as f32;
                let b = chunk[i + 2] as f32;
                let nr = r * m0 + g * m1 + b * m2 + m4;
                let ng = g * m6 + m9;
                let nb = b * m12 + m14;
                let mut fr = nr.clamp(0.0, 255.0);
                let mut fg = ng.clamp(0.0, 255.0);
                let mut fb = nb.clamp(0.0, 255.0);
                if strength < 1.0 {
                    let o = &orig[row_i * row + i..];
                    fr = fr * strength + o[0] as f32 * (1.0 - strength);
                    fg = fg * strength + o[1] as f32 * (1.0 - strength);
                    fb = fb * strength + o[2] as f32 * (1.0 - strength);
                }
                if dehaze > 0.0 {
                    let gray = (fr + fg + fb) / 3.0;
                    let k = 1.0 + dehaze.clamp(0.0, 1.0);
                    fr = (gray + (fr - gray) * k).clamp(0.0, 255.0);
                    fg = (gray + (fg - gray) * k).clamp(0.0, 255.0);
                    fb = (gray + (fb - gray) * k).clamp(0.0, 255.0);
                }
                chunk[i] = fr as u8;
                chunk[i + 1] = fg as u8;
                chunk[i + 2] = fb as u8;
                i += 3;
            }
        });
}

#[allow(clippy::needless_range_loop)]
pub fn interpolate_matrices(samples: &[(usize, Matrix)], frame_count: usize) -> Vec<Matrix> {
    if samples.is_empty() {
        return vec![
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0
            ];
            frame_count
        ];
    }
    let indices: Vec<f64> = samples.iter().map(|(i, _)| *i as f64).collect();
    let mut out = vec![[0.0f32; 20]; frame_count];
    for k in 0..20 {
        let vals: Vec<f64> = samples.iter().map(|(_, m)| m[k] as f64).collect();
        for f in 0..frame_count {
            let x = f as f64;
            if x <= indices[0] {
                out[f][k] = vals[0] as f32;
            } else if x >= *indices.last().unwrap() {
                out[f][k] = *vals.last().unwrap() as f32;
            } else {
                let mut j = 0;
                while j + 1 < indices.len() && indices[j + 1] < x {
                    j += 1;
                }
                let x0 = indices[j];
                let x1 = indices[j + 1];
                let v0 = vals[j];
                let v1 = vals[j + 1];
                let t = (x - x0) / (x1 - x0);
                out[f][k] = (v0 + (v1 - v0) * t) as f32;
            }
        }
    }
    out
}

pub fn get_filter_matrix_from_rgb(
    width: u32,
    height: u32,
    rgb: &[u8],
    cfg: &GradeConfig,
) -> Matrix {
    let img = RgbImage::from_raw(width, height, rgb.to_vec())
        .unwrap_or_else(|| RgbImage::new(width, height));
    get_filter_matrix(&img, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn small_img() -> RgbImage {
        let mut img = RgbImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                img.put_pixel(x, y, image::Rgb([(x * 4) as u8, (y * 4) as u8, 120]));
            }
        }
        img
    }

    #[test]
    fn matrix_deterministic() {
        let cfg = GradeConfig::default();
        let img = small_img();
        let a = get_filter_matrix(&img, &cfg);
        let b = get_filter_matrix(&img, &cfg);
        assert_eq!(a, b);
    }

    #[test]
    fn apply_filter_strength_zero_identity() {
        let cfg = GradeConfig::default();
        let img = small_img();
        let matrix = get_filter_matrix(&img, &cfg);
        let mut px = img.into_raw();
        let orig = px.clone();
        apply_filter(&mut px, 64, &matrix, 0.0, 0.0);
        assert_eq!(px, orig);
    }

    #[test]
    fn apply_filter_no_panic_odd_size() {
        let cfg = GradeConfig::default();
        let img = RgbImage::new(3, 5);
        let matrix = get_filter_matrix(&img, &cfg);
        let mut px = img.into_raw();
        apply_filter(&mut px, 3, &matrix, 1.0, 0.0);
    }
}
