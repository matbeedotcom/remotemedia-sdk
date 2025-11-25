# Video Test Samples

This directory contains test video files for integration testing.

## Files

- `sample_720p30.raw` - Raw YUV420P video (1280x720, 30fps) - placeholder
- `sample_vp8.webm` - Encoded VP8 video sample - placeholder

## Usage

These files are used by integration tests in `tests/integration/test_video_*.rs`

To generate actual test samples:
```bash
# Generate raw YUV420P sample (10 frames of solid gray)
ffmpeg -f lavfi -i color=gray:s=1280x720:d=0.333 -pix_fmt yuv420p sample_720p30.raw

# Generate VP8 encoded sample
ffmpeg -i sample_720p30.raw -c:v libvpx -b:v 1M sample_vp8.webm
```
