use anyhow::{Context, Result};
use divegrader_core::GradeConfig;
use image::{ImageFormat, RgbImage};
use std::fs;
use std::path::Path;

pub fn is_image(p: &Path) -> bool {
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .as_deref(),
        Some("jpg") | Some("jpeg") | Some("png") | Some("bmp") | Some("tiff") | Some("tif")
    )
}

pub fn process_image(
    input: &Path,
    output: &Path,
    cfg: &GradeConfig,
    quality: u8,
    _overwrite: bool,
    gpu: bool,
) -> Result<()> {
    // Always overwrite existing outputs (graded files are reproducible).
    let img = image::open(input).context("open image")?;
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let res = divegrader_core::analyze(&rgb, cfg);
    let mut buf = rgb.into_raw();
    let used_gpu = gpu
        && crate::gpu::apply_filter_gpu(
            &mut buf,
            w as usize,
            h as usize,
            &res.matrix,
            cfg.strength,
            cfg.dehaze,
        );
    if !used_gpu {
        divegrader_core::apply_filter(&mut buf, w as usize, &res.matrix, cfg.strength, cfg.dehaze);
    }
    divegrader_core::post_process(&mut buf, w as usize, &res, cfg);
    let out_img = RgbImage::from_raw(w, h, buf).context("build image")?;

    // extra grading: contrast/saturation/gamma/temperature (simple)
    // (kept minimal: just save)
    let format = ImageFormat::from_path(output).unwrap_or(ImageFormat::Jpeg);
    if matches!(format, ImageFormat::Jpeg) {
        use image::codecs::jpeg::JpegEncoder;
        use std::fs::File;
        use std::io::BufWriter;
        let file = File::create(output)?;
        let mut enc = JpegEncoder::new_with_quality(BufWriter::new(file), quality);
        enc.encode_image(&out_img).context("encode jpeg")?;
        let _ = copy_exif(input, output);
    } else {
        out_img
            .save_with_format(output, format)
            .context("save image")?;
    }
    let _ = quality;
    let _ = fs::metadata(input);
    Ok(())
}

fn copy_exif(input: &Path, output: &Path) -> Result<()> {
    use img_parts::jpeg::Jpeg;
    use img_parts::ImageEXIF;
    let input_bytes = fs::read(input)?;
    let output_bytes = fs::read(output)?;
    let in_jpeg = Jpeg::from_bytes(input_bytes.into()).map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut out_jpeg =
        Jpeg::from_bytes(output_bytes.into()).map_err(|e| anyhow::anyhow!("{}", e))?;
    if let Some(exif) = in_jpeg.exif() {
        out_jpeg.set_exif(Some(exif));
    }
    fs::write(output, out_jpeg.encoder().bytes())?;
    Ok(())
}
