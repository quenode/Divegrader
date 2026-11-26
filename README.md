# 🤿 divegrader

**divegrader** is a fast, lightweight command-line tool written in Rust for automatic underwater photo and video color correction.

Water absorbs red light quickly at depth, leaving scuba, freediving, and snorkeling footage heavily tinted blue or green. `divegrader` restores natural colors by compensating for the lost red channel and dynamically normalizing color histograms—without stripping your audio or EXIF/GPS metadata.

## ✨ Features

* **Automatic Color Restoration:** Restores red tones and removes underwater haze for both photos and videos.
* **Dynamic Video Grading:** Samples frames across the video and smoothly interpolates color matrices as depth and lighting change.
* **Fast & Lightweight:** Built in Rust for multi-threaded image processing and low memory usage.
* **Audio & Metadata Safe:** Uses FFmpeg under the hood to keep original audio tracks, timestamps, and camera metadata intact.
* **Open Source:** Free to use, no watermarks, no limits (AGPL-3.0).

## 🗺️ Roadmap

- [x] Core underwater color correction algorithm (photos)
- [ ] FFmpeg video pipeline with audio & metadata preservation
- [ ] Hardware-accelerated video encoding (NVENC / QSV / VAAPI)
- [ ] **Immich companion service** (automatic grading of tagged albums & stacking with originals)

## 🚀 Usage (Planned)

```bash
# Grade a single photo
divegrader photo.jpg

# Grade a video
divegrader dive_video.mp4 -o graded_video.mp4
