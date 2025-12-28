//! WAV file parsing utilities
//!
//! Provides functions to read WAV files and convert them to RuntimeData::Audio.

use anyhow::{Context, Result};
use std::io::{Read, Seek, SeekFrom};

/// WAV file header information
#[derive(Debug, Clone)]
pub struct WavHeader {
    /// Number of channels (1 = mono, 2 = stereo)
    pub channels: u16,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Bits per sample (8, 16, 24, 32)
    pub bits_per_sample: u16,
    /// Audio format (1 = PCM, 3 = IEEE float)
    pub audio_format: u16,
    /// Size of audio data in bytes
    pub data_size: u32,
}

/// Read WAV header from a reader
pub fn read_wav_header<R: Read + Seek>(reader: &mut R) -> Result<WavHeader> {
    let mut buffer = [0u8; 4];

    // Read RIFF header
    reader.read_exact(&mut buffer)?;
    if &buffer != b"RIFF" {
        anyhow::bail!("Not a valid WAV file: missing RIFF header");
    }

    // Skip file size
    reader.read_exact(&mut buffer)?;

    // Read WAVE format
    reader.read_exact(&mut buffer)?;
    if &buffer != b"WAVE" {
        anyhow::bail!("Not a valid WAV file: missing WAVE format");
    }

    let mut fmt_found = false;
    let mut audio_format = 0u16;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut data_size = 0u32;

    // Parse chunks until we find fmt and data
    loop {
        // Read chunk ID
        if reader.read_exact(&mut buffer).is_err() {
            break;
        }
        let chunk_id = buffer;

        // Read chunk size
        let mut size_buf = [0u8; 4];
        reader
            .read_exact(&mut size_buf)
            .context("Failed to read chunk size")?;
        let chunk_size = u32::from_le_bytes(size_buf);

        match &chunk_id {
            b"fmt " => {
                // Format chunk
                let mut fmt_buf = [0u8; 16];
                reader
                    .read_exact(&mut fmt_buf)
                    .context("Failed to read fmt chunk")?;

                audio_format = u16::from_le_bytes([fmt_buf[0], fmt_buf[1]]);
                channels = u16::from_le_bytes([fmt_buf[2], fmt_buf[3]]);
                sample_rate = u32::from_le_bytes([fmt_buf[4], fmt_buf[5], fmt_buf[6], fmt_buf[7]]);
                // Skip byte rate (4 bytes) and block align (2 bytes)
                bits_per_sample = u16::from_le_bytes([fmt_buf[14], fmt_buf[15]]);

                // Skip any extra format bytes
                if chunk_size > 16 {
                    reader.seek(SeekFrom::Current((chunk_size - 16) as i64))?;
                }

                fmt_found = true;
            }
            b"data" => {
                data_size = chunk_size;
                // Don't seek past data - we want to read it next
                break;
            }
            _ => {
                // Skip unknown chunk
                reader.seek(SeekFrom::Current(chunk_size as i64))?;
            }
        }
    }

    if !fmt_found {
        anyhow::bail!("WAV file missing fmt chunk");
    }

    Ok(WavHeader {
        channels,
        sample_rate,
        bits_per_sample,
        audio_format,
        data_size,
    })
}

/// Read WAV audio data as f32 samples
///
/// Converts PCM data to normalized f32 samples in range [-1.0, 1.0]
/// Handles streaming WAV files where data_size may be 0xFFFFFFFF (unknown)
pub fn read_wav_samples<R: Read>(reader: &mut R, header: &WavHeader, total_data_len: Option<usize>) -> Result<Vec<f32>> {
    let bytes_per_sample = header.bits_per_sample / 8;

    // Handle streaming WAV files where data_size is 0xFFFFFFFF (unknown)
    // In this case, read all remaining data
    let buffer = if header.data_size == 0xFFFFFFFF || header.data_size == 0 {
        // Read all remaining data
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        tracing::debug!("Streaming WAV: read {} bytes of audio data", buf.len());
        buf
    } else if let Some(available) = total_data_len {
        // Use the smaller of declared size or available data
        let read_size = std::cmp::min(header.data_size as usize, available);
        let mut buf = vec![0u8; read_size];
        reader.read_exact(&mut buf)?;
        buf
    } else {
        let mut buf = vec![0u8; header.data_size as usize];
        reader.read_exact(&mut buf)?;
        buf
    };

    let num_samples = buffer.len() / bytes_per_sample as usize;
    let mut samples = Vec::with_capacity(num_samples);

    match (header.audio_format, header.bits_per_sample) {
        // PCM 8-bit unsigned
        (1, 8) => {
            for byte in buffer {
                // Convert from unsigned 0-255 to signed -1.0 to 1.0
                samples.push((byte as f32 - 128.0) / 128.0);
            }
        }
        // PCM 16-bit signed
        (1, 16) => {
            for chunk in buffer.chunks_exact(2) {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                samples.push(sample as f32 / 32768.0);
            }
        }
        // PCM 24-bit signed
        (1, 24) => {
            for chunk in buffer.chunks_exact(3) {
                // Sign-extend 24-bit to 32-bit
                let sample = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], 0]);
                let sample = if sample & 0x800000 != 0 {
                    sample | 0xFF000000u32 as i32
                } else {
                    sample
                };
                samples.push(sample as f32 / 8388608.0);
            }
        }
        // PCM 32-bit signed
        (1, 32) => {
            for chunk in buffer.chunks_exact(4) {
                let sample = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                samples.push(sample as f32 / 2147483648.0);
            }
        }
        // IEEE float 32-bit
        (3, 32) => {
            for chunk in buffer.chunks_exact(4) {
                let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                samples.push(sample);
            }
        }
        // IEEE float 64-bit
        (3, 64) => {
            for chunk in buffer.chunks_exact(8) {
                let sample = f64::from_le_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
                ]);
                samples.push(sample as f32);
            }
        }
        _ => {
            anyhow::bail!(
                "Unsupported WAV format: audio_format={}, bits_per_sample={}",
                header.audio_format,
                header.bits_per_sample
            );
        }
    }

    Ok(samples)
}

/// Parse a complete WAV file from bytes
///
/// Returns (samples, sample_rate, channels)
/// Handles streaming WAV files where the data size may be unknown (0xFFFFFFFF)
pub fn parse_wav(data: &[u8]) -> Result<(Vec<f32>, u32, u16)> {
    let mut cursor = std::io::Cursor::new(data);
    let header = read_wav_header(&mut cursor).with_context(|| {
        format!(
            "Failed to read WAV header (data size: {} bytes)",
            data.len()
        )
    })?;

    tracing::debug!(
        "WAV header: format={}, channels={}, rate={}, bits={}, data_size={}",
        header.audio_format,
        header.channels,
        header.sample_rate,
        header.bits_per_sample,
        header.data_size
    );

    // Calculate remaining data after header
    let remaining_data = data.len() - cursor.position() as usize;

    let samples = read_wav_samples(&mut cursor, &header, Some(remaining_data)).with_context(|| {
        format!(
            "Failed to read WAV samples (declared {} bytes, {} remaining at position {})",
            header.data_size,
            remaining_data,
            cursor.position()
        )
    })?;

    tracing::debug!(
        "Parsed WAV: {} samples, {}Hz, {} channels, {} bits",
        samples.len(),
        header.sample_rate,
        header.channels,
        header.bits_per_sample
    );

    Ok((samples, header.sample_rate, header.channels))
}

/// Check if data starts with WAV header
pub fn is_wav(data: &[u8]) -> bool {
    data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_wav() {
        let wav_header = b"RIFF\x00\x00\x00\x00WAVE";
        assert!(is_wav(wav_header));

        let not_wav = b"Not a WAV file";
        assert!(!is_wav(not_wav));
    }
}
