//! End-to-end test for streaming pipeline execution
//!
//! This example demonstrates streaming input → streaming output through a pipeline.

use remotemedia_runtime_core::{
    data::{PixelFormat, RuntimeData},
    manifest::Manifest,
    transport::{PipelineRunner, StreamSession, TransportData},
};
use std::sync::Arc;
use tokio::time::{timeout, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 End-to-End Streaming Pipeline Test\n");

    // Create pipeline runner
    let runner = Arc::new(PipelineRunner::new()?);
    println!("✓ Created PipelineRunner");

    // Create a simple manifest with VideoFlip node
    let manifest_json = r#"{
        "version": "v1",
        "metadata": {
            "name": "video-flip-test",
            "description": "Test pipeline with VideoFlip node"
        },
        "nodes": [
            {
                "id": "flip",
                "node_type": "VideoFlip",
                "params": {
                    "direction": "vertical"
                }
            }
        ],
        "connections": []
    }"#;

    let manifest: Manifest = serde_json::from_str(manifest_json)?;
    let manifest_arc = Arc::new(manifest);
    println!("✓ Created manifest with VideoFlip node");

    // Create streaming session
    let mut session = runner.create_stream_session(manifest_arc.clone()).await?;
    println!("✓ Created streaming session: {}", session.session_id());

    // Test 1: Send single frame through stream
    println!("\n📹 Test 1: Single frame streaming");
    {
        // Create test video frame (2x2 RGB24)
        let input_pixels = vec![
            255, 0, 0, // red
            0, 255, 0, // green
            0, 0, 255, // blue
            255, 255, 255, // white
        ];

        let input_data = RuntimeData::Video {
            pixel_data: input_pixels,
            width: 2,
            height: 2,
            format: remotemedia_runtime_core::data::PixelFormat::Rgb24,
            codec: None,
            frame_number: 0,
            timestamp_us: 0,
            is_keyframe: true,
            stream_id: None,
        };

        // Send input (wrap in TransportData)
        session.send_input(TransportData::new(input_data)).await?;
        println!("  → Sent video frame (2x2 RGB24)");

        // Receive output with timeout
        let output = timeout(Duration::from_secs(5), session.recv_output())
            .await
            .map_err(|_| "Timeout waiting for output")??
            .ok_or("No output received")?
            .data;

        // Verify output
        if let RuntimeData::Video {
            pixel_data,
            width,
            height,
            format,
            ..
        } = output
        {
            assert_eq!(width, 2);
            assert_eq!(height, 2);
            assert_eq!(format, PixelFormat::Rgb24);
            assert_eq!(pixel_data.len(), 12);

            // After vertical flip:
            // Top row: blue, white
            // Bottom row: red, green
            assert_eq!(pixel_data[0..3], [0, 0, 255]); // blue
            assert_eq!(pixel_data[3..6], [255, 255, 255]); // white
            assert_eq!(pixel_data[6..9], [255, 0, 0]); // red
            assert_eq!(pixel_data[9..12], [0, 255, 0]); // green

            println!("  ✓ Received flipped video frame");
            println!("  ✓ Verified pixel data is correctly flipped");
        } else {
            return Err("Expected Video output".into());
        }
    }

    // Test 2: Stream multiple frames
    println!("\n📹 Test 2: Multiple frame streaming");
    {
        const NUM_FRAMES: usize = 5;

        for i in 0..NUM_FRAMES {
            // Create frame with frame number encoded in first pixel
            let mut input_pixels = vec![0u8; 12]; // 2x2 RGB24
            input_pixels[0] = i as u8; // Encode frame number in red channel

            let input_data = RuntimeData::Video {
                pixel_data: input_pixels,
                width: 2,
                height: 2,
                format: PixelFormat::Rgb24,
                codec: None,
                frame_number: i as u64,
                timestamp_us: i as u64 * 33333, // ~30 FPS
                is_keyframe: true,
                stream_id: None,
            };

            session.send_input(TransportData::new(input_data)).await?;
        }
        println!("  → Sent {} video frames", NUM_FRAMES);

        // Receive all outputs
        for i in 0..NUM_FRAMES {
            let output = timeout(Duration::from_secs(5), session.recv_output())
                .await
                .map_err(|_| format!("Timeout waiting for frame {}", i))??
                .ok_or(format!("No output for frame {}", i))?
                .data;

            if let RuntimeData::Video { frame_number, .. } = output {
                assert_eq!(frame_number, i as u64);
            } else {
                return Err("Expected Video output".into());
            }
        }
        println!("  ✓ Received all {} frames in order", NUM_FRAMES);
    }

    // Test 3: Verify I420 format support
    println!("\n📹 Test 3: I420 format streaming");
    {
        // Create 4x4 I420 frame
        let mut input_pixels = vec![0u8; 24]; // 4x4 I420 (16 Y + 4 U + 4 V)

        // Y plane gradient
        for i in 0..16 {
            input_pixels[i] = (i * 16) as u8;
        }

        // U and V planes
        input_pixels[16..20].copy_from_slice(&[100, 110, 120, 130]);
        input_pixels[20..24].copy_from_slice(&[200, 210, 220, 230]);

        let input_data = RuntimeData::Video {
            pixel_data: input_pixels.clone(),
            width: 4,
            height: 4,
            format: PixelFormat::I420,
            codec: None,
            is_keyframe: true,
            stream_id: None,
            frame_number: 100,
            timestamp_us: 3000000,
            arrival_ts_us: None,
        };

        session.send_input(TransportData::new(input_data)).await?;
        println!("  → Sent I420 frame (4x4)");

        let output = timeout(Duration::from_secs(5), session.recv_output())
            .await
            .map_err(|_| "Timeout waiting for I420 output")??
            .ok_or("No I420 output received")?
            .data;

        if let RuntimeData::Video {
            pixel_data,
            width,
            height,
            format,
            ..
        } = output
        {
            assert_eq!(width, 4);
            assert_eq!(height, 4);
            assert_eq!(format, PixelFormat::I420);
            assert_eq!(pixel_data.len(), 24);

            // Verify Y plane is flipped
            assert_eq!(pixel_data[0..4], [192, 208, 224, 240]); // Last row becomes first
            println!("  ✓ Received flipped I420 frame");
            println!("  ✓ Verified Y plane is correctly flipped");
        } else {
            return Err("Expected Video output".into());
        }
    }

    // Test 4: Pipeline throughput
    println!("\n📊 Test 4: Pipeline throughput");
    {
        const THROUGHPUT_FRAMES: usize = 100;
        let start = std::time::Instant::now();

        // Send frames
        for i in 0..THROUGHPUT_FRAMES {
            let input_data = RuntimeData::Video {
                pixel_data: vec![0u8; 12],
                width: 2,
                height: 2,
                format: PixelFormat::Rgb24,
                codec: None,
                is_keyframe: true,
                stream_id: None,
                frame_number: i as u64,
                timestamp_us: i as u64 * 33333,
                arrival_ts_us: None,
            };
            session.send_input(TransportData::new(input_data)).await?;
        }

        // Receive all
        for _ in 0..THROUGHPUT_FRAMES {
            timeout(Duration::from_secs(10), session.recv_output())
                .await
                .map_err(|_| "Throughput test timeout")??;
        }

        let elapsed = start.elapsed();
        let fps = THROUGHPUT_FRAMES as f64 / elapsed.as_secs_f64();

        println!(
            "  ✓ Processed {} frames in {:.2}s",
            THROUGHPUT_FRAMES,
            elapsed.as_secs_f64()
        );
        println!("  ✓ Throughput: {:.2} FPS", fps);
        println!(
            "  ✓ Avg latency: {:.2}ms per frame",
            elapsed.as_millis() as f64 / THROUGHPUT_FRAMES as f64
        );
    }

    // Clean shutdown
    session.close().await?;
    println!("\n✓ Session closed successfully");

    println!("\n🎉 All end-to-end tests passed!");
    Ok(())
}
