//! Shared audio sample extraction utilities
//!
//! This module provides common audio format conversion functions used by
//! both URL-based demuxers and AVIO-based demuxers.

/// Audio sample format constants (from libavutil/samplefmt.h)
#[allow(dead_code)]
pub mod sample_formats {
    pub const AV_SAMPLE_FMT_NONE: i32 = -1;
    pub const AV_SAMPLE_FMT_U8: i32 = 0;
    pub const AV_SAMPLE_FMT_S16: i32 = 1;
    pub const AV_SAMPLE_FMT_S32: i32 = 2;
    pub const AV_SAMPLE_FMT_FLT: i32 = 3;
    pub const AV_SAMPLE_FMT_DBL: i32 = 4;
    pub const AV_SAMPLE_FMT_U8P: i32 = 5;
    pub const AV_SAMPLE_FMT_S16P: i32 = 6;
    pub const AV_SAMPLE_FMT_S32P: i32 = 7;
    pub const AV_SAMPLE_FMT_FLTP: i32 = 8;
    pub const AV_SAMPLE_FMT_DBLP: i32 = 9;

    /// Check if a sample format is planar
    pub fn is_planar(format: i32) -> bool {
        matches!(format, AV_SAMPLE_FMT_U8P | AV_SAMPLE_FMT_S16P | AV_SAMPLE_FMT_S32P | AV_SAMPLE_FMT_FLTP | AV_SAMPLE_FMT_DBLP)
    }
}

/// Convert packed audio samples to f32
///
/// Handles conversion from various packed sample formats to f32.
/// Returns an empty Vec if the format is unsupported.
///
/// # Safety
/// - `data` must point to valid memory of at least `total_samples` elements of the correct type
pub unsafe fn convert_packed_samples_to_f32(
    data: *const u8,
    format: i32,
    total_samples: usize,
) -> Vec<f32> {
    use sample_formats::*;

    match format {
        AV_SAMPLE_FMT_FLT => {
            let ptr = data as *const f32;
            std::slice::from_raw_parts(ptr, total_samples).to_vec()
        }
        AV_SAMPLE_FMT_S16 => {
            let ptr = data as *const i16;
            std::slice::from_raw_parts(ptr, total_samples)
                .iter()
                .map(|&s| s as f32 / 32768.0)
                .collect()
        }
        AV_SAMPLE_FMT_S32 => {
            let ptr = data as *const i32;
            std::slice::from_raw_parts(ptr, total_samples)
                .iter()
                .map(|&s| s as f32 / 2147483648.0)
                .collect()
        }
        AV_SAMPLE_FMT_DBL => {
            let ptr = data as *const f64;
            std::slice::from_raw_parts(ptr, total_samples)
                .iter()
                .map(|&s| s as f32)
                .collect()
        }
        AV_SAMPLE_FMT_U8 => {
            let ptr = data;
            std::slice::from_raw_parts(ptr, total_samples)
                .iter()
                .map(|&s| (s as f32 - 128.0) / 128.0)
                .collect()
        }
        _ => {
            tracing::warn!("Unsupported packed audio format: {}", format);
            Vec::new()
        }
    }
}

/// Convert planar audio samples to interleaved f32
///
/// Handles conversion from various planar sample formats to interleaved f32.
/// Returns an empty Vec if the format is unsupported.
///
/// # Safety
/// - `plane_data` must contain `channels` valid pointers, each pointing to `nb_samples` elements
pub unsafe fn convert_planar_samples_to_f32(
    plane_data: &[*const u8],
    format: i32,
    nb_samples: usize,
) -> Vec<f32> {
    use sample_formats::*;

    let channels = plane_data.len();
    let total_samples = nb_samples * channels;
    let mut result = Vec::with_capacity(total_samples);

    match format {
        AV_SAMPLE_FMT_FLTP => {
            for i in 0..nb_samples {
                for ch in 0..channels {
                    let plane = plane_data[ch] as *const f32;
                    result.push(*plane.add(i));
                }
            }
        }
        AV_SAMPLE_FMT_S16P => {
            for i in 0..nb_samples {
                for ch in 0..channels {
                    let plane = plane_data[ch] as *const i16;
                    result.push(*plane.add(i) as f32 / 32768.0);
                }
            }
        }
        AV_SAMPLE_FMT_S32P => {
            for i in 0..nb_samples {
                for ch in 0..channels {
                    let plane = plane_data[ch] as *const i32;
                    result.push(*plane.add(i) as f32 / 2147483648.0);
                }
            }
        }
        AV_SAMPLE_FMT_DBLP => {
            for i in 0..nb_samples {
                for ch in 0..channels {
                    let plane = plane_data[ch] as *const f64;
                    result.push(*plane.add(i) as f32);
                }
            }
        }
        AV_SAMPLE_FMT_U8P => {
            for i in 0..nb_samples {
                for ch in 0..channels {
                    let plane = plane_data[ch];
                    result.push((*plane.add(i) as f32 - 128.0) / 128.0);
                }
            }
        }
        _ => {
            tracing::warn!("Unsupported planar audio format: {}", format);
        }
    }

    result
}

/// Convert FFmpeg error code to string
pub fn ffmpeg_error_string(errnum: i32) -> String {
    extern "C" {
        fn av_strerror(errnum: i32, errbuf: *mut i8, errbuf_size: usize) -> i32;
    }
    
    let mut buf = [0u8; 256];
    unsafe {
        av_strerror(errnum, buf.as_mut_ptr() as *mut i8, buf.len());
    }
    String::from_utf8_lossy(&buf).trim_end_matches('\0').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_planar() {
        use sample_formats::*;
        assert!(!is_planar(AV_SAMPLE_FMT_FLT));
        assert!(!is_planar(AV_SAMPLE_FMT_S16));
        assert!(is_planar(AV_SAMPLE_FMT_FLTP));
        assert!(is_planar(AV_SAMPLE_FMT_S16P));
    }

    #[test]
    fn test_convert_packed_s16() {
        let samples: [i16; 4] = [0, 16384, -16384, 32767];
        unsafe {
            let result = convert_packed_samples_to_f32(
                samples.as_ptr() as *const u8,
                sample_formats::AV_SAMPLE_FMT_S16,
                4,
            );
            assert_eq!(result.len(), 4);
            assert!((result[0] - 0.0).abs() < 0.001);
            assert!((result[1] - 0.5).abs() < 0.001);
            assert!((result[2] - -0.5).abs() < 0.001);
        }
    }

    #[test]
    fn test_convert_packed_float() {
        let samples: [f32; 3] = [0.0, 0.5, -0.5];
        unsafe {
            let result = convert_packed_samples_to_f32(
                samples.as_ptr() as *const u8,
                sample_formats::AV_SAMPLE_FMT_FLT,
                3,
            );
            assert_eq!(result, samples);
        }
    }
}
