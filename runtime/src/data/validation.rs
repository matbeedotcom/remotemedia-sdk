// Validation functions for data types
// Feature: 004-generic-streaming

use crate::grpc_service::generated::{VideoFrame, TensorBuffer, TextBuffer, PixelFormat, TensorDtype};
use crate::Error;

/// Validate video frame pixel data matches dimensions and format
///
/// Checks that pixel_data.len() == width * height * bytes_per_pixel(format)
pub fn validate_video_frame(frame: &VideoFrame) -> Result<(), Error> {
    if frame.width == 0 || frame.height == 0 {
        return Err(Error::InvalidInput {
            message: "Video frame dimensions must be > 0".into(),
            node_id: String::new(),
            context: format!("width={}, height={}", frame.width, frame.height),
        });
    }

    let expected_bytes = match PixelFormat::try_from(frame.format) {
        Ok(PixelFormat::Rgb24) => (frame.width * frame.height * 3) as usize,
        Ok(PixelFormat::Rgba32) => (frame.width * frame.height * 4) as usize,
        Ok(PixelFormat::Yuv420p) => {
            // Y plane: width*height, U plane: (width/2)*(height/2), V plane: same as U
            ((frame.width * frame.height * 3) / 2) as usize
        },
        Ok(PixelFormat::Gray8) => (frame.width * frame.height) as usize,
        _ => {
            return Err(Error::InvalidInput {
                message: "Unknown pixel format".into(),
                node_id: String::new(),
                context: format!("format={}", frame.format),
            });
        }
    };

    if frame.pixel_data.len() != expected_bytes {
        return Err(Error::InvalidInput {
            message: "Video frame pixel data size mismatch".into(),
            node_id: String::new(),
            context: format!(
                "Expected {} bytes for {}x{} {:?}, got {} bytes",
                expected_bytes, frame.width, frame.height,
                PixelFormat::try_from(frame.format).unwrap_or(PixelFormat::Unspecified),
                frame.pixel_data.len()
            ),
        });
    }

    Ok(())
}

/// Validate tensor data length matches shape and dtype
///
/// Checks that data.len() == shape.product() * bytes_per_element(dtype)
pub fn validate_tensor_size(tensor: &TensorBuffer) -> Result<(), Error> {
    let expected_elements: u64 = tensor.shape.iter().product();

    let bytes_per_element = match TensorDtype::try_from(tensor.dtype) {
        Ok(TensorDtype::F32) | Ok(TensorDtype::I32) => 4,
        Ok(TensorDtype::F16) => 2,
        Ok(TensorDtype::I8) | Ok(TensorDtype::U8) => 1,
        _ => {
            return Err(Error::InvalidInput {
                message: "Unknown tensor dtype".into(),
                node_id: String::new(),
                context: format!("dtype={}", tensor.dtype),
            });
        }
    };

    let expected_bytes = expected_elements * bytes_per_element;

    if tensor.data.len() != expected_bytes as usize {
        return Err(Error::InvalidInput {
            message: "Tensor data size mismatch".into(),
            node_id: String::new(),
            context: format!(
                "Expected {} bytes for shape {:?} with dtype {:?}, got {} bytes",
                expected_bytes, tensor.shape,
                TensorDtype::try_from(tensor.dtype).unwrap_or(TensorDtype::Unspecified),
                tensor.data.len()
            ),
        });
    }

    Ok(())
}

/// Validate text buffer is valid UTF-8
///
/// Returns the validated string if successful
pub fn validate_text_buffer(text_buf: &TextBuffer) -> Result<String, Error> {
    String::from_utf8(text_buf.text_data.clone()).map_err(|e| {
        Error::InvalidInput {
            message: "Invalid UTF-8 in text buffer".into(),
            node_id: String::new(),
            context: format!(
                "Invalid UTF-8 at byte offset {}, encoding={}",
                e.utf8_error().valid_up_to(),
                text_buf.encoding
            ),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_video_frame() {
        let frame = VideoFrame {
            pixel_data: vec![0u8; 640 * 480 * 3],
            width: 640,
            height: 480,
            format: PixelFormat::Rgb24 as i32,
            frame_number: 0,
            timestamp_us: 0,
        };
        assert!(validate_video_frame(&frame).is_ok());
    }

    #[test]
    fn test_invalid_video_frame_size() {
        let frame = VideoFrame {
            pixel_data: vec![0u8; 100], // Too small
            width: 640,
            height: 480,
            format: PixelFormat::Rgb24 as i32,
            frame_number: 0,
            timestamp_us: 0,
        };
        assert!(validate_video_frame(&frame).is_err());
    }

    #[test]
    fn test_zero_dimensions() {
        let frame = VideoFrame {
            pixel_data: vec![],
            width: 0,
            height: 0,
            format: PixelFormat::Rgb24 as i32,
            frame_number: 0,
            timestamp_us: 0,
        };
        assert!(validate_video_frame(&frame).is_err());
    }

    #[test]
    fn test_valid_tensor() {
        let tensor = TensorBuffer {
            data: vec![0u8; 512 * 4], // 512 elements * 4 bytes (F32)
            shape: vec![512],
            dtype: TensorDtype::F32 as i32,
            layout: String::new(),
        };
        assert!(validate_tensor_size(&tensor).is_ok());
    }

    #[test]
    fn test_invalid_tensor_size() {
        let tensor = TensorBuffer {
            data: vec![0u8; 100], // Too small
            shape: vec![512],
            dtype: TensorDtype::F32 as i32,
            layout: String::new(),
        };
        assert!(validate_tensor_size(&tensor).is_err());
    }

    #[test]
    fn test_multidimensional_tensor() {
        // 3x224x224 image tensor (F32)
        let expected_bytes = 3 * 224 * 224 * 4;
        let tensor = TensorBuffer {
            data: vec![0u8; expected_bytes],
            shape: vec![3, 224, 224],
            dtype: TensorDtype::F32 as i32,
            layout: "NCHW".into(),
        };
        assert!(validate_tensor_size(&tensor).is_ok());
    }

    #[test]
    fn test_valid_utf8() {
        let text_buf = TextBuffer {
            text_data: "Hello, 世界!".as_bytes().to_vec(),
            encoding: "utf-8".into(),
            language: "en".into(),
        };
        let result = validate_text_buffer(&text_buf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, 世界!");
    }

    #[test]
    fn test_invalid_utf8() {
        let text_buf = TextBuffer {
            text_data: vec![0xFF, 0xFE, 0xFD], // Invalid UTF-8
            encoding: "utf-8".into(),
            language: String::new(),
        };
        assert!(validate_text_buffer(&text_buf).is_err());
    }
}
