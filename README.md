# 🤿 divegrader

**divegrader** is a fast, lightweight command-line tool written in Rust for automatic underwater photo and video color correction.

Water absorbs red light quickly at depth, leaving scuba, freediving, and snorkeling footage heavily tinted blue or green. `divegrader` restores natural colors by compensating for the lost red channel and dynamically normalizing color histograms—without stripping your audio or EXIF/GPS metadata.

## ✨ Features

* **Automatic Color Restoration:** Restores red tones and removes underwater haze for both photos and videos.
* **Dynamic Video Grading:** Samples frames across the video and smoothly interpolates color matrices as depth and lighting change.
* **Fast & Lightweight:** Built in Rust for multi-threaded image processing and low memory usage.
* **Audio & Metadata Safe:** Uses FFmpeg under the hood to keep original audio tracks, timestamps, and camera metadata intact.
* **Open Source:** Free to use, no watermarks, no limits (AGPL-3.0).

## 📚 Documentation

Full documentation with every option explained: **[DOCS.md](DOCS.md)**

## 🗺️ Roadmap

- [x] Core underwater color correction algorithm (photos)
- [ ] FFmpeg video pipeline with audio & metadata preservation
- [ ] Hardware-accelerated video encoding (NVENC / QSV / VAAPI)
- [ ] **Immich companion service** (automatic grading of tagged albums & stacking with originals)

## 🚀 Usage

```bash
# Grade a single photo (writes photo_graded.jpg next to input)
divegrader photo.jpg

# Grade a video, keep audio/metadata
divegrader dive_video.mp4 -o graded_video.mp4

# Batch a folder, 8 threads, H.265, CRF 18
divegrader ./dives -o ./graded --jobs 8 --codec x265 --crf 18

# Recursive with glob, keep folder structure
divegrader -r examples/videos -o out/ --hw nvenc

# Tune grading
divegrader photo.jpg --strength 0.8 --blue-magic 1.3 --preset deep

# Stabilize video while grading
divegrader dive_video.mp4 --hw nvenc --stabilize

# Save/reuse a config
divegrader photo.jpg --save-config grade.toml
divegrader ./dives --config grade.toml
```
