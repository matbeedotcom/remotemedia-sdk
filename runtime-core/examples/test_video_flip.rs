use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::data::PixelFormat;
use remotemedia_runtime_core::nodes::streaming_node::AsyncStreamingNode;
use remotemedia_runtime_core::nodes::video_flip::{FlipDirection, VideoFlipConfig, VideoFlipNode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing VideoFlipNode...\n");

    // Test 1: Vertical flip
    println!("Test 1: Vertical flip");
    let config = VideoFlipConfig {
        direction: FlipDirection::Vertical,
    };
    let node = VideoFlipNode::new(config);

    // Create a simple 2x2 RGB24 image
    // Top row: red, green
    // Bottom row: blue, white
    let input = vec![
        255, 0, 0, // red
        0, 255, 0, // green
        0, 0, 255, // blue
        255, 255, 255, // white
    ];

    let input_data = RuntimeData::Video {
        pixel_data: input,
        width: 2,
        height: 2,
        format: PixelFormat::Rgb24,
        codec: None,
        frame_number: 0,
        timestamp_us: 0,
        is_keyframe: true,
        stream_id: None,
        arrival_ts_us: None,
    };

    let output = node.process(input_data).await?;

    if let RuntimeData::Video { pixel_data, .. } = output {
        // After vertical flip:
        // Top row: blue, white
        // Bottom row: red, green
        assert_eq!(pixel_data[0..3], [0, 0, 255]); // blue
        assert_eq!(pixel_data[3..6], [255, 255, 255]); // white
        assert_eq!(pixel_data[6..9], [255, 0, 0]); // red
        assert_eq!(pixel_data[9..12], [0, 255, 0]); // green
        println!("  ✓ Vertical flip test passed!");
    } else {
        panic!("Expected Video output");
    }

    // Test 2: Horizontal flip
    println!("\nTest 2: Horizontal flip");
    let config = VideoFlipConfig {
        direction: FlipDirection::Horizontal,
    };
    let node = VideoFlipNode::new(config);

    let input = vec![
        255, 0, 0, // red
        0, 255, 0, // green
        0, 0, 255, // blue
        255, 255, 255, // white
    ];

    let input_data = RuntimeData::Video {
        pixel_data: input,
        width: 2,
        height: 2,
        format: PixelFormat::Rgb24,
        codec: None,
        frame_number: 0,
        timestamp_us: 0,
        is_keyframe: true,
        stream_id: None,
        arrival_ts_us: None,
    };

    let output = node.process(input_data).await?;

    if let RuntimeData::Video { pixel_data, .. } = output {
        // After horizontal flip:
        // Top row: green, red
        // Bottom row: white, blue
        assert_eq!(pixel_data[0..3], [0, 255, 0]); // green
        assert_eq!(pixel_data[3..6], [255, 0, 0]); // red
        assert_eq!(pixel_data[6..9], [255, 255, 255]); // white
        assert_eq!(pixel_data[9..12], [0, 0, 255]); // blue
        println!("  ✓ Horizontal flip test passed!");
    }

    // Test 3: I420 format
    println!("\nTest 3: I420 vertical flip");
    let config = VideoFlipConfig {
        direction: FlipDirection::Vertical,
    };
    let node = VideoFlipNode::new(config);

    // Create a simple 4x4 I420 image (minimum size for I420)
    // Y plane: 4x4 = 16 bytes
    // U plane: 2x2 = 4 bytes
    // V plane: 2x2 = 4 bytes
    let mut input = vec![0u8; 24];

    // Set Y plane with gradient
    for i in 0..16 {
        input[i] = i as u8;
    }

    // U and V planes
    input[16..20].copy_from_slice(&[100, 101, 102, 103]);
    input[20..24].copy_from_slice(&[200, 201, 202, 203]);

    let input_data = RuntimeData::Video {
        pixel_data: input,
        width: 4,
        height: 4,
        format: PixelFormat::I420,
        codec: None,
        is_keyframe: true,
        stream_id: None,
        frame_number: 0,
        timestamp_us: 0,
        arrival_ts_us: None,
    };

    let output = node.process(input_data).await?;

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

        // Y plane should be flipped vertically
        assert_eq!(pixel_data[0..4], [12, 13, 14, 15]); // Last row becomes first
        assert_eq!(pixel_data[12..16], [0, 1, 2, 3]); // First row becomes last
        println!("  ✓ I420 flip test passed!");
    }

    println!("\n✓ All VideoFlipNode tests passed!");
    Ok(())
}
