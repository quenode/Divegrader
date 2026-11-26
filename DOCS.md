# divegrader — Documentation

**divegrader** is a fast, open-source (AGPL-3.0) command-line tool written in Rust for
automatic underwater photo and video color correction. Water absorbs red light quickly
at depth, leaving scuba, freediving and snorkeling footage heavily tinted blue or green.
divegrader restores natural colors by compensating for the lost red channel and
dynamically normalizing color histograms — without stripping audio or metadata.

- Works on Windows, macOS and Linux.
- One small binary. No GUI required (a TUI dashboard is included).
- Uses FFmpeg under the hood for video decode/encode, audio and metadata.

---

## Table of contents

1. [Installation](#installation)
2. [Quick start](#quick-start)
3. [General options](#general-options)
4. [Output options](#output-options)
5. [Grading options](#grading-options)
6. [Video encoding options](#video-encoding-options)
7. [Hardware acceleration](#hardware-acceleration)
8. [Stabilization](#stabilization)
9. [UI options (TUI / progress)](#ui-options-tui--progress)
10. [Config file](#config-file)
11. [How the algorithm works](#how-the-algorithm-works)
12. [Presets and profiles](#presets-and-profiles)
13. [Examples](#examples)

---

## Installation

### Build from source

```bash
git clone <repo>
cd divegrader
cargo build --release
# binary: target/release/divegrader
```

Requirements:

- Rust 1.85+ (stable)
- FFmpeg 8.x recommended in `PATH` (`ffmpeg` and `ffprobe`). FFmpeg 6.1 works too,
  but modern NVIDIA GPUs (RTX 40/50 series) need FFmpeg 8.x for CUDA decode and NVENC.
  Static builds: https://github.com/FFmpeg-Builds/FFmpeg-Builds
- Optional for `--gpu`: any Vulkan/Metal/DX12 GPU (uses wgpu).
- Optional for NVENC: NVIDIA GPU + driver.

---

## Quick start

```bash
# Grade a photo (writes photo_graded.jpg next to the original)
divegrader photo.jpg

# Grade a video, keep audio and metadata
divegrader dive.mp4 -o graded_dive.mp4

# Grade everything in a folder, output into ./graded, 8 parallel jobs
divegrader ./dives -o ./graded --jobs 8

# Recursive folder processing, keeping the folder structure
divegrader -r ./dives -o ./graded
```

File types are detected automatically:

- **Images**: jpg, jpeg, png, bmp, tiff, tif
- **Videos**: mp4, mkv, mov, avi, webm

---

## General options

| Option | Description |
| --- | --- |
| `<INPUTS>...` | One or more input files, globs, or directories. |
| `-r, --recursive` | When an input is a directory, walk it recursively. Without this flag only files directly inside the directory are picked up. |
| `--jobs <N>` | Number of files processed in parallel (default: all CPU cores; `0` = auto). |
| `-v, --verbose` | Print more information while running. |
| `--version` | Print version. |
| `-h, --help` | Print help. |

---

## Output options

| Option | Default | Description |
| --- | --- | --- |
| `-o, --output <PATH>` | *(next to input)* | Output file **or** directory. If the path has no file extension (or is an existing directory) it is treated as a directory. When used as a directory, the relative path of each input is preserved under it. |
| `--suffix <STR>` | `graded` | Suffix appended to auto-generated names: `photo.jpg` → `photo_graded.jpg`. |
| `--overwrite` | *(always on)* | Accepted for compatibility. divegrader always overwrites existing outputs; graded files are reproducible. |
| `--quality <1-100>` | `95` | JPEG quality for output photos. |

Examples:

```bash
divegrader a.jpg                                # a_graded.jpg (same folder)
divegrader a.jpg -o b.jpg                       # explicit file name
divegrader a.jpg -o outdir/                     # outdir/a_graded.jpg
divegrader -r ./dives -o ./graded               # ./graded/<relative-path>/<name>_graded.mp4
```

---

## Grading options

These tune the color-correction algorithm (see [How the algorithm works](#how-the-algorithm-works)).

| Option | Default | Range | Description |
| --- | --- | --- | --- |
| `--strength <F>` | `1.0` | `0.0–1.0` | Blend between the original (`0.0`) and fully corrected (`1.0`) image. |
| `--min-avg-red <F>` | `60` | `0–255` | Target for the average red level after hue shift. Higher = stronger red restoration. |
| `--max-hue-shift <F>` | `120` | `0–360` | Maximum hue rotation (degrees) the algorithm is allowed to apply. |
| `--blue-magic <F>` | `1.2` | `0.0–3.0` | Multiplier for the blue channel's contribution to the restored red channel. Lower values are better for green water. |
| `--threshold-ratio <F>` | `2000` | `>0` | Histogram threshold: `1/N` of pixels. Higher = more aggressive histogram stretching. |
| `--sample-seconds <F>` | `2` | `>0` | *(video)* How often (seconds) a filter matrix is sampled before interpolation. |
| `--water <MODE>` | `auto` | `auto`, `blue`, `green` | Water type. `auto` detects from the green/blue channel balance. `green` reduces blue gain for green (lake/cave) water. |
| `--depth <MODE>` | `auto` | `auto`, `shallow`, `mid`, `deep` | Depth category. `auto` estimates absorption from the red-channel deficit and picks the matching target. `deep` restores red more aggressively. |
| `--profile <NAME>` | `default` | `default`, `reef`, `wreck`, `cave`, `night` | Ready-made combinations (see [Presets and profiles](#presets-and-profiles)). Applied on top of everything else. |
| `--grade-preset <NAME>` | `default` | `default`, `shallow`, `deep`, `green-water`, `red-boost` | Legacy quick presets that only tweak `min_avg_red` and `blue_magic`. |

Config-file equivalents (TOML): `strength`, `min_avg_red`, `max_hue_shift`,
`blue_magic`, `threshold_ratio`, `sample_seconds`, `water`, `depth`, `profile`,
`preset`, plus `contrast`, `saturation`, `gamma`, `temperature` (applied on the GPU path).

---

## Video encoding options

| Option | Default | Description |
| --- | --- | --- |
| `--codec <C>` | `x265` | Output codec family: `x265` (H.265/HEVC), `x264` (H.264/AVC), `vp9`, or any FFmpeg encoder name (e.g. `libx265`). With NVENC, `x265`→`hevc_nvenc`, `x264`→`h264_nvenc`. |
| `--crf <N>` | `18` | Quality: lower = better/larger. H.264/H.265 use CRF; NVENC maps it to `-cq` (VBR). Common values: `18` (visually lossless-ish), `22–24` (good balance), `28` (small files). |
| `--preset <P>` | `medium` | Encoder speed preset (e.g. `fast`, `medium`, `slow`, NVENC `p1`–`p7`). |
| `--no-audio` | off | Drop audio. By default the original audio tracks are copied 1:1. |
| `--stabilize` | off | Stabilize the video while grading (see [Stabilization](#stabilization)). |

Metadata: by default the original file's metadata is copied to the output
(`-map_metadata 1`), plus `+faststart` for fast web playback.

---

## Hardware acceleration

| Option | Default | Description |
| --- | --- | --- |
| `--hw <MODE>` | `auto` | Hardware encode/decode mode. `auto` detects NVENC and uses CUDA decode if available; `nvenc` forces it; `cpu` disables it. |
| `--gpu` | off | Run the color-correction math on the GPU (wgpu compute shader: Vulkan/Metal/DX12). Falls back to CPU automatically when no adapter is available. |

Notes:

- NVENC and CUDA decode need an NVIDIA GPU + driver; FFmpeg 8.x is recommended
  (older FFmpeg may report "Hardware is lacking required capabilities" on RTX 40/50 cards).
- `--gpu` also applies contrast/saturation/gamma/temperature grading from the config.
- The color filter is light per pixel; the GPU mostly helps with very large
  (4K+) footage and by freeing CPU cores for FFmpeg.

---

## Stabilization

`--stabilize` adds two FFmpeg passes around the grading pipeline:

1. **vidstabdetect** — analyzes the whole video (downscaled to 640×360 for speed)
   and writes a motion-transform file.
2. **vidstabtransform** — applies the transforms to the graded 4K frames before
   encoding (`zoom=5:smoothing=15` defaults).

Example:

```bash
divegrader shaky_dive.mp4 -o stable.mp4 --stabilize --hw nvenc
```

Expect roughly 3–5x longer processing while stabilizing, because vidstabtransform
runs on the CPU at full resolution. A gyro-data-based stabilizer (for cameras like
the Insta360 Ace Pro that record gyro metadata) is planned for a future release.

---

## UI options (TUI / progress)

| Option | Description |
| --- | --- |
| `--tui` | Force the interactive terminal dashboard. |
| `--silent` | Disable all UI/progress output (scripts, CI). |
| `-q, --quiet` | Minimal output. |

The TUI starts automatically when stdout is a terminal and shows:

- Overall and per-file progress bars (gradient colors, Catppuccin Mocha palette)
- Animated spinner and scrollable log with OK/FAIL entries
- Live system stats: CPU %, RAM, GPU name/driver/utilization/VRAM/temperature
- Vertical CPU and GPU load bars
- Press `q` to abort: processing stops and all FFmpeg subprocesses are terminated.

---

## Config file

Save or load a grading configuration as TOML:

```bash
# save current settings
divegrader photo.jpg --save-config grade.toml

# reuse them later
divegrader ./dives -r -o graded --config grade.toml
```

Example `grade.toml`:

```toml
min_avg_red = 60.0
max_hue_shift = 120.0
blue_magic = 1.2
sample_seconds = 2.0
threshold_ratio = 2000.0
strength = 1.0
contrast = 1.0
saturation = 1.0
gamma = 1.0
temperature = 0.0
preset = "default"
water = "auto"
depth = "auto"
profile = "default"
```

CLI flags override nothing automatically — when `--config` is given, its values
are used as-is (except `--profile`, which is re-applied on top).

---

## How the algorithm works

Ported from [dive-color-corrector](https://github.com/bornfree/dive-color-corrector):

1. **Downscale** the image/frame to 256×256.
2. **Hue shift**: rotate colors so the average red level reaches `min_avg_red`
   (depth-aware target when `depth=auto`), capped at `max_hue_shift` degrees.
3. **Red restoration**: replace the red channel with the sum of the hue-shifted
   RGB channels (scaled by `blue_magic` for the blue contribution).
4. **Histogram normalization**: per channel, find the widest gap in the
   low-frequency histogram range and stretch that range to 0–255.
5. **Filter matrix**: the above is baked into a 4×5 matrix (20 floats).
6. **Apply**: every pixel goes through `matrix × RGB + offset`, clamped to 0–255,
   then blended with the original by `strength`.

**Videos** additionally:

- Frames are sampled every `sample_seconds` *and* at detected scene changes.
- The 20 matrix coefficients are smoothed over neighboring samples and linearly
  interpolated per frame, so grading adapts smoothly as depth/lighting change.

**Auto depth detection**: measures the red-channel deficit
(`1 - avg_red / max(avg_green, avg_blue)`):

| Deficit | Depth | min_avg_red target |
| --- | --- | --- |
| < 0.15 | shallow | 50 |
| 0.15–0.35 | mid | 55 |
| 0.35–0.60 | deep-ish | 65 |
| > 0.60 | deep | 75 |

**Auto water detection**: if green ≥ blue the water is treated as green
(`blue_magic` ≈ 1.05), otherwise blue (`blue_magic` ≈ 1.25).

---

## Presets and profiles

### `--grade-preset` (simple tweaks)

| Preset | Effect |
| --- | --- |
| `default` | No change (algorithm defaults). |
| `shallow` | `min_avg_red=50`, `blue_magic=1.1` |
| `deep` | `min_avg_red=70`, `blue_magic=1.3` |
| `green-water` | `min_avg_red=65`, `blue_magic=1.0` |
| `red-boost` | `min_avg_red=80`, `blue_magic=1.4` |

### `--profile` (ready-made combinations)

| Profile | What it sets |
| --- | --- |
| `default` | No change. |
| `reef` | `water=blue`, `depth=auto` |
| `wreck` | `water=green`, `depth=mid` |
| `cave` | `water=blue`, `depth=deep`, `contrast=1.05` |
| `night` | `min_avg_red=50`, `blue_magic=1.0`, `contrast=1.1`, `saturation=1.1` |

Profiles are applied **after** presets and config values, so they win.

---

## Examples

```bash
# Photos
divegrader img1.jpg img2.jpg img3.jpg
divegrader *.jpg -o graded_photos/ --quality 92

# Videos
divegrader dive.mp4 --hw nvenc                        # NVENC H.265, CUDA decode
divegrader dive.mp4 --codec x264 --crf 20             # CPU H.264
divegrader dive.mp4 --hw nvenc --stabilize            # stabilize + grade
divegrader dive.mp4 --hw cpu --codec vp9 --crf 30     # web-friendly VP9

# Tuning
divegrader dive.mp4 --water green --depth mid
divegrader cave.mp4 --profile cave
divegrader night.mp4 --profile night --strength 0.8
divegrader photo.jpg --blue-magic 1.3 --min-avg-red 70

# Batch + GPU + TUI
divegrader -r ./dives -o ./graded --jobs 8 --hw nvenc --gpu --tui

# Scripts / CI
divegrader -r ./in -o ./out --silent
```

---

## License

AGPL-3.0. See `LICENSE`. The color-correction algorithm is adapted from
[dive-color-corrector](https://github.com/bornfree/dive-color-corrector)
(MIT) — see `examples/dive-color-corrector/LICENSE`.
