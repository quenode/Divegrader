# Changelog

All notable changes to divegrader will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-11-27

### Added
- `--green-gain-cap` (default 1.0): green channel is never amplified.
- Regional water-type detection: blue vs green water is judged from the top quarter of the frame (the water column), so green reefs no longer skew detection.

### Fixed
- Green oversaturation on shallow green-reef footage; reef grades toward brown and open water stays blue.

## [0.3.2] - 2026-11-27

### Fixed
- Green oversaturation on shallow reef footage: green channel gain now capped at 1.0 by default (`--green-gain-cap`).
- Water type (blue vs green) now detected from the top quarter of the frame instead of the whole image, so green reefs no longer cause misclassification.
- `--white-balance` defaults to `off` and `--dehaze` to `0.0` (opt-in features).
- Default `--max-gain` raised to 10.0 to preserve original behavior.

## [0.3.1] - 2026-11-27

### Fixed
- Green channel oversaturation: clamp per-channel histogram gain (default 2.0).
- Conservative grey-world pre-pass (higher dominance threshold, capped scale).
- Added `--max-gain` and `--green-scale` knobs for manual tuning.

## [0.3.0] - 2026-11-27

### Added
- Grey-world white-balance pre-pass (`--white-balance auto|off`).
- Dehaze pass (`--dehaze 0.0-1.0`) to reduce underwater haze.
- Median keyframe mode (`--keyframes interpolate|median`) for robust video grading.
- New grading options exposed in help: `--water`, `--depth`, `--profile`.
- Gain clamping (`--max-gain`, default 2.0) to prevent green oversaturation.
- `--green-scale` for manual green channel attenuation.
- Conservative grey-world pre-pass (threshold raised to 1.5x dominance).

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
