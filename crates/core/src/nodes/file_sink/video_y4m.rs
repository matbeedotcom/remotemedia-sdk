//! `VideoFileWriterNode` — writes incoming `RuntimeData::Video`
//! frames to a Y4M file on disk.
//!
//! Y4M (YUV4MPEG2) is the simplest standard video container that
//! ffmpeg / mpv read directly: a one-line ASCII header followed by
//! `FRAME\n` markers + raw planar YUV bytes per frame. No
//! per-chunk size fields means no Drop-time fixup is needed.
//!
//! ## Format
//!
//! `C420jpeg` (YUV420p, JPEG-range chroma siting). The renderer
//! emits RGB24 (per `Live2DRenderNode`); we convert to YUV420p
//! per frame using the standard BT.601 coefficients. Subsequent
//! ffmpeg conversion to H.264 / VP9 / etc. has no quality loss
//! beyond the RGB→YUV step.
//!
//! ## Lifecycle
//!
//! - First Video frame: writes Y4M header (captures width / height
//!   / fps from the first frame's metadata).
//! - Each Video frame: writes `FRAME\n` + Y plane + Cb plane + Cr
//!   plane.
//! - On `Drop`: simply closes the file (no fixup needed — Y4M has
//!   no length fields).
//!
//! ## Inferring fps
//!
//! `RuntimeData::Video` carries `timestamp_us` per frame. We
//! capture the first frame's pts as t0, then default to 30 fps
//! for the header. Mismatches between header fps and actual frame
//! cadence are not a correctness issue — Y4M readers tolerate
//! variable frame rate; the header is a pacing hint.

use crate::data::RuntimeData;
use crate::error::Result;
use crate::nodes::AsyncStreamingNode;
use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for [`VideoFileWriterNode`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFileWriterConfig {
    /// Output Y4M path. Parent directory is auto-created.
    pub output_path: PathBuf,
    /// Frame rate to stamp in the header. Defaults to 30; affects
    /// how downstream players pace playback. Real frame cadence is
    /// determined by the producer, not this header.
    #[serde(default = "default_fps")]
    pub fps: u32,
}

fn default_fps() -> u32 {
    30
}

/// Streaming node that writes incoming Video frames to a `.y4m`
/// file. Pass-through: emits the same Video frames it received so
/// it can sit on a tap edge without breaking the data flow.
pub struct VideoFileWriterNode {
    config: VideoFileWriterConfig,
    state: Arc<Mutex<WriterState>>,
}

struct WriterState {
    file: Option<File>,
    width: u32,
    height: u32,
    /// Frames written so far. Diagnostic only; Y4M doesn't store it.
    frames_written: u64,
}

impl VideoFileWriterNode {
    pub fn new(config: VideoFileWriterConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(WriterState {
                file: None,
                width: 0,
                height: 0,
                frames_written: 0,
            })),
            config,
        }
    }
}

impl std::fmt::Debug for VideoFileWriterNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.state.lock();
        f.debug_struct("VideoFileWriterNode")
            .field("output_path", &self.config.output_path)
            .field("fps", &self.config.fps)
            .field("frames_written", &s.frames_written)
            .field("dims", &(s.width, s.height))
            .finish()
    }
}

#[async_trait]
impl AsyncStreamingNode for VideoFileWriterNode {
    fn node_type(&self) -> &str {
        "VideoFileWriterNode"
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData> {
        write_one(&self.state, &self.config, &data)?;
        Ok(data)
    }
}

fn write_one(
    state: &Mutex<WriterState>,
    config: &VideoFileWriterConfig,
    data: &RuntimeData,
) -> Result<()> {
    let RuntimeData::Video { pixel_data, width, height, format, .. } = data else {
        return Ok(()); // pass-through non-video
    };
    use crate::data::video::PixelFormat;
    if !matches!(format, PixelFormat::Rgb24) {
        tracing::warn!(
            "VideoFileWriterNode: dropping frame with format {:?} \
             (expected Rgb24)",
            format
        );
        return Ok(());
    }
    let expected_len = (*width as usize) * (*height as usize) * 3;
    if pixel_data.len() != expected_len {
        tracing::warn!(
            "VideoFileWriterNode: dropping frame with bad payload \
             length {} (expected {} for {}x{} RGB24)",
            pixel_data.len(),
            expected_len,
            width,
            height
        );
        return Ok(());
    }

    let mut s = state.lock();
    if s.file.is_none() {
        if let Some(parent) = config.output_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut f = File::create(&config.output_path).map_err(|e| {
            crate::Error::Execution(format!(
                "VideoFileWriterNode: open {:?}: {e}",
                config.output_path
            ))
        })?;
        write_y4m_header(&mut f, *width, *height, config.fps)?;
        s.file = Some(f);
        s.width = *width;
        s.height = *height;
    }

    if *width != s.width || *height != s.height {
        tracing::warn!(
            "VideoFileWriterNode: dropping frame with mismatched dimensions \
             ({}x{} vs initial {}x{})",
            width, height, s.width, s.height
        );
        return Ok(());
    }

    let f = s.file.as_mut().expect("file open after init");
    f.write_all(b"FRAME\n").map_err(|e| {
        crate::Error::Execution(format!("Y4M FRAME marker: {e}"))
    })?;
    let yuv = rgb24_to_yuv420p(pixel_data, *width as usize, *height as usize);
    f.write_all(&yuv).map_err(|e| {
        crate::Error::Execution(format!("Y4M frame data: {e}"))
    })?;
    s.frames_written += 1;
    Ok(())
}

fn write_y4m_header(f: &mut File, width: u32, height: u32, fps: u32) -> Result<()> {
    // Standard Y4M header. Fields:
    //   W{width}    — frame width
    //   H{height}   — frame height
    //   F{n}:{d}    — frame rate as a rational (n/d)
    //   Ip          — progressive interlacing
    //   A1:1        — square pixel aspect
    //   C420jpeg    — YUV420p, JPEG-range chroma siting
    let header = format!(
        "YUV4MPEG2 W{} H{} F{}:1 Ip A1:1 C420jpeg\n",
        width, height, fps
    );
    f.write_all(header.as_bytes()).map_err(|e| {
        crate::Error::Execution(format!("Y4M header: {e}"))
    })?;
    Ok(())
}

/// BT.601 RGB24 → YUV420p (planar) conversion. Y is full
/// resolution; Cb / Cr are 2×2 sub-sampled. Padding rows / cols
/// for odd dimensions get the last valid row / col duplicated —
/// not a concern for our typical even-resolution renders.
///
/// Output layout: `[Y_plane][Cb_plane][Cr_plane]`.
fn rgb24_to_yuv420p(rgb: &[u8], w: usize, h: usize) -> Vec<u8> {
    let y_size = w * h;
    let chroma_w = (w + 1) / 2;
    let chroma_h = (h + 1) / 2;
    let chroma_size = chroma_w * chroma_h;
    let mut out = vec![0u8; y_size + 2 * chroma_size];
    let (y_plane, rest) = out.split_at_mut(y_size);
    let (cb_plane, cr_plane) = rest.split_at_mut(chroma_size);

    // Y plane: per-pixel.
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 3;
            let r = rgb[i] as f32;
            let g = rgb[i + 1] as f32;
            let b = rgb[i + 2] as f32;
            let y_val = 0.299 * r + 0.587 * g + 0.114 * b;
            y_plane[y * w + x] = y_val.round().clamp(0.0, 255.0) as u8;
        }
    }

    // Cb / Cr planes: 2×2 average.
    for cy in 0..chroma_h {
        for cx in 0..chroma_w {
            let mut r_sum = 0.0f32;
            let mut g_sum = 0.0f32;
            let mut b_sum = 0.0f32;
            let mut count = 0.0f32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let py = (cy * 2 + dy).min(h - 1);
                    let px = (cx * 2 + dx).min(w - 1);
                    let i = (py * w + px) * 3;
                    r_sum += rgb[i] as f32;
                    g_sum += rgb[i + 1] as f32;
                    b_sum += rgb[i + 2] as f32;
                    count += 1.0;
                }
            }
            let r = r_sum / count;
            let g = g_sum / count;
            let b = b_sum / count;
            let cb = 128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b;
            let cr = 128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b;
            cb_plane[cy * chroma_w + cx] = cb.round().clamp(0.0, 255.0) as u8;
            cr_plane[cy * chroma_w + cx] = cr.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::video::PixelFormat;

    fn rgb_frame(w: u32, h: u32, fill: u8) -> RuntimeData {
        RuntimeData::Video {
            pixel_data: vec![fill; (w * h * 3) as usize],
            width: w,
            height: h,
            format: PixelFormat::Rgb24,
            codec: None,
            frame_number: 0,
            timestamp_us: 0,
            is_keyframe: true,
            stream_id: None,
            arrival_ts_us: None,
        }
    }

    #[tokio::test]
    async fn writes_a_valid_y4m_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.y4m");
        let node = VideoFileWriterNode::new(VideoFileWriterConfig {
            output_path: path.clone(),
            fps: 30,
        });
        node.process(rgb_frame(4, 4, 128)).await.unwrap();
        node.process(rgb_frame(4, 4, 64)).await.unwrap();
        node.process(rgb_frame(4, 4, 200)).await.unwrap();
        drop(node);

        let bytes = std::fs::read(&path).unwrap();
        // Header: starts with "YUV4MPEG2 ".
        assert!(bytes.starts_with(b"YUV4MPEG2 "));
        // Expected sizes: each 4x4 frame = 16 Y + 4 Cb + 4 Cr = 24 bytes
        // + 6-byte "FRAME\n" marker. 3 frames total = 90 bytes of payload.
        let expected_payload = 3 * (24 + 6);
        let header_end = bytes.iter().position(|&b| b == b'\n').unwrap() + 1;
        assert_eq!(
            bytes.len() - header_end,
            expected_payload,
            "expected {} bytes of payload after the header",
            expected_payload
        );
        // First frame marker.
        assert_eq!(&bytes[header_end..header_end + 6], b"FRAME\n");
    }

    #[tokio::test]
    async fn drops_mismatched_dim_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.y4m");
        let node = VideoFileWriterNode::new(VideoFileWriterConfig {
            output_path: path.clone(),
            fps: 30,
        });
        node.process(rgb_frame(4, 4, 128)).await.unwrap();
        // 8x8 — different dimensions, dropped.
        node.process(rgb_frame(8, 8, 64)).await.unwrap();
        node.process(rgb_frame(4, 4, 200)).await.unwrap();
        drop(node);

        let bytes = std::fs::read(&path).unwrap();
        let header_end = bytes.iter().position(|&b| b == b'\n').unwrap() + 1;
        // 2 frames × 30 bytes each = 60 bytes of payload.
        assert_eq!(bytes.len() - header_end, 60);
    }

    #[test]
    fn rgb_to_yuv_white_is_full_luma() {
        let rgb = vec![255u8; 2 * 2 * 3]; // 2x2 white
        let yuv = rgb24_to_yuv420p(&rgb, 2, 2);
        // Y plane = 4 bytes, all should be 255 (full luma).
        for &y in &yuv[..4] {
            assert!(y >= 254, "white pixel Y should be ~255, got {}", y);
        }
        // Cb / Cr should be ~128 (chroma neutral).
        assert!(yuv[4].abs_diff(128) < 2, "white Cb should be ~128, got {}", yuv[4]);
        assert!(yuv[5].abs_diff(128) < 2, "white Cr should be ~128, got {}", yuv[5]);
    }

    #[test]
    fn rgb_to_yuv_pure_red_chroma_check() {
        let rgb = vec![255u8, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0]; // 2x2 red
        let yuv = rgb24_to_yuv420p(&rgb, 2, 2);
        // Y for pure red ≈ 0.299 * 255 ≈ 76.
        assert!(yuv[0].abs_diff(76) < 2);
        // Cb for pure red ≈ 128 - 0.169 * 255 ≈ 85.
        assert!(yuv[4].abs_diff(85) < 2);
        // Cr for pure red ≈ 128 + 0.5 * 255 = 255.5 → 255.
        assert!(yuv[5] > 250, "Cr for red should be ≥250, got {}", yuv[5]);
    }

    #[tokio::test]
    async fn non_video_input_is_passthrough_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("never.y4m");
        let node = VideoFileWriterNode::new(VideoFileWriterConfig {
            output_path: path.clone(),
            fps: 30,
        });
        node.process(RuntimeData::Text("hi".into())).await.unwrap();
        drop(node);
        assert!(!path.exists(), "file should not be created without video input");
    }
}
