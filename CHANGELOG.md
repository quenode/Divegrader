# Changelog

All notable changes to divegrader will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-11-26

### Added
- Smarter automatic color grading: depth-aware red restoration and water-type
  detection (blue vs green water), controlled by new `water` and `depth`
  config options (`auto` by default).
- `--stabilize` flag: video stabilization via FFmpeg vidstab
  (vidstabdetect + vidstabtransform).
- Scene-aware keyframe sampling (samples at scene changes, not just fixed
  intervals) and temporal smoothing of grading matrices to avoid flicker.
- `--recursive` / `-r` flag for recursive directory input.
- Batch processing of files and directories with parallel jobs (`--jobs`).
- Recursive output keeps the input directory structure under the output folder.
- GPU color processing backend (`--gpu`, wgpu) with packed transfers,
  buffer reuse, and batched frames, with automatic CPU fallback.
- Hardware video encoding (`--hw auto|cpu|nvenc`) with NVENC support
  (H.264/H.265) and CUDA-accelerated decoding.
- Interactive TUI (`--tui`) with progress bars, per-file status, logs, and
  live system stats (CPU, RAM, GPU, drivers) in the Catppuccin Mocha palette.
- `--silent` flag to disable all UI output for scripts/CI.
- `--dehaze <0.0-1.0>`: boost saturation in hazy mid/shadow regions.
- `--white-balance auto|off`: grey-world white-balance pre-pass.
- `--keyframes interpolate|median`: median keyframe mode (robust to transient objects).
- `--strength`, `--min-avg-red`, `--max-hue-shift`, `--blue-magic`,
  `--threshold-ratio`, `--grade-preset` grading options.
- TOML config files: `--config` and `--save-config`.
- Output overwrite by default (existing files are replaced).

### Changed
- Video pipeline now runs decode, color grading, and encode concurrently
  (pipelined worker threads), roughly 2.5x faster on 4K footage.
- Video analysis pass decodes downscaled (256x256) frames for faster sampling.
- Updated FFmpeg requirement: FFmpeg 8.x recommended (tested with
  FFmpeg-Builds 8.1) for CUDA decode + NVENC on modern NVIDIA GPUs.

### Fixed
- GPU dispatch overflow for large frames (2D dispatch groups).
- Buffer size / binding limit crashes on 4K frames.
- Terminal restoration and cursor visibility after TUI exit.
- `q` in the TUI now aborts processing and terminates FFmpeg subprocesses.
- Leftover FFmpeg processes after cancellation.

## [0.1.0] - 2026-11-26

### Added
- Initial release: Rust CLI for underwater photo and video color correction.
- Core algorithm ported from dive-color-corrector (hue-shift red restoration,
  histogram normalization, interpolated filter matrices).
- Image and video processing with audio/metadata preservation via FFmpeg.
- EXIF preservation for JPEG photos.
