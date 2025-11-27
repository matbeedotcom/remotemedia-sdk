//! Output validation helpers for WebRTC E2E testing
//!
//! Provides assertion helpers for validating pipeline outputs.

use super::test_client::{ClientQualityMetrics, ReceivedAudioChunk, ReceivedVideoFrame, TestClient};
use super::{HarnessError, HarnessResult};
use std::time::Duration;
use tracing::info;

/// Output validator for test assertions
pub struct OutputValidator;

impl OutputValidator {
    /// Create a new output validator
    pub fn new() -> Self {
        Self
    }

    // ========================================================================
    // Wait for Output
    // ========================================================================

    /// Wait for audio output from a client
    ///
    /// # Arguments
    ///
    /// * `client` - Test client to receive from
    /// * `timeout` - Maximum time to wait
    ///
    /// # Returns
    ///
    /// Raw audio data (Opus encoded)
    pub async fn expect_audio_output(
        &self,
        client: &TestClient,
        timeout: Duration,
    ) -> HarnessResult<Vec<u8>> {
        client.wait_for_audio(timeout).await
    }

    /// Wait for video output from a client
    ///
    /// # Arguments
    ///
    /// * `client` - Test client to receive from
    /// * `timeout` - Maximum time to wait
    ///
    /// # Returns
    ///
    /// Received video frame
    pub async fn expect_video_output(
        &self,
        client: &TestClient,
        timeout: Duration,
    ) -> HarnessResult<ReceivedVideoFrame> {
        client.wait_for_video(timeout).await
    }

    /// Wait for at least N audio packets
    pub async fn expect_min_audio_packets(
        &self,
        client: &TestClient,
        min_packets: u32,
        timeout: Duration,
    ) -> HarnessResult<Vec<ReceivedAudioChunk>> {
        let deadline = tokio::time::Instant::now() + timeout;

        while tokio::time::Instant::now() < deadline {
            let audio = client.get_received_audio().await;
            if audio.len() >= min_packets as usize {
                return Ok(audio);
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Err(HarnessError::Timeout(format!(
            "Expected at least {} audio packets, timed out",
            min_packets
        )))
    }

    /// Wait for at least N video frames
    pub async fn expect_min_video_frames(
        &self,
        client: &TestClient,
        min_frames: usize,
        timeout: Duration,
    ) -> HarnessResult<Vec<ReceivedVideoFrame>> {
        let deadline = tokio::time::Instant::now() + timeout;

        while tokio::time::Instant::now() < deadline {
            let frames = client.get_received_video().await;
            if frames.len() >= min_frames {
                return Ok(frames);
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Err(HarnessError::Timeout(format!(
            "Expected at least {} video frames, timed out",
            min_frames
        )))
    }

    // ========================================================================
    // Audio Assertions
    // ========================================================================

    /// Assert that two audio signals are similar within tolerance
    ///
    /// Compares using normalized mean squared error (NMSE)
    ///
    /// # Arguments
    ///
    /// * `a` - First audio signal
    /// * `b` - Second audio signal
    /// * `tolerance` - Maximum allowed NMSE (e.g., 0.01 for 1% error)
    pub fn assert_audio_similar(&self, a: &[f32], b: &[f32], tolerance: f32) -> HarnessResult<()> {
        // Allow length difference up to 10%
        let len_diff = (a.len() as f32 - b.len() as f32).abs() / a.len().max(1) as f32;
        if len_diff > 0.1 {
            return Err(HarnessError::ValidationError(format!(
                "Audio length mismatch: {} vs {} samples ({:.1}% difference)",
                a.len(),
                b.len(),
                len_diff * 100.0
            )));
        }

        // Compare overlapping portion
        let min_len = a.len().min(b.len());
        if min_len == 0 {
            return Err(HarnessError::ValidationError(
                "Cannot compare empty audio signals".to_string(),
            ));
        }

        // Calculate normalized mean squared error
        let mse: f32 = a[..min_len]
            .iter()
            .zip(b[..min_len].iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            / min_len as f32;

        // Normalize by signal power
        let power_a: f32 = a[..min_len].iter().map(|x| x.powi(2)).sum::<f32>() / min_len as f32;
        let power_b: f32 = b[..min_len].iter().map(|x| x.powi(2)).sum::<f32>() / min_len as f32;
        let avg_power = (power_a + power_b) / 2.0;

        let nmse = if avg_power > 0.0 {
            mse / avg_power
        } else {
            mse // Both signals are near-silent
        };

        if nmse > tolerance {
            return Err(HarnessError::ValidationError(format!(
                "Audio signals differ: NMSE = {:.4} (tolerance: {:.4})",
                nmse, tolerance
            )));
        }

        Ok(())
    }

    /// Assert that audio has non-zero energy (not silence)
    pub fn assert_audio_not_silent(&self, samples: &[f32]) -> HarnessResult<()> {
        let rms = self.calculate_rms(samples);

        // RMS below -60dB is effectively silent
        let silence_threshold = 0.001; // ~-60dB

        if rms < silence_threshold {
            return Err(HarnessError::ValidationError(format!(
                "Audio is silent: RMS = {:.6} (threshold: {:.6})",
                rms, silence_threshold
            )));
        }

        Ok(())
    }

    /// Assert that audio is silent (below threshold)
    pub fn assert_audio_silent(&self, samples: &[f32]) -> HarnessResult<()> {
        let rms = self.calculate_rms(samples);
        let silence_threshold = 0.001;

        if rms > silence_threshold {
            return Err(HarnessError::ValidationError(format!(
                "Expected silence but audio has energy: RMS = {:.6}",
                rms
            )));
        }

        Ok(())
    }

    /// Calculate RMS (root mean square) of audio samples
    pub fn calculate_rms(&self, samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }

        let sum_squares: f32 = samples.iter().map(|x| x.powi(2)).sum();
        (sum_squares / samples.len() as f32).sqrt()
    }

    /// Assert audio samples are within valid range [-1.0, 1.0]
    pub fn assert_audio_in_range(&self, samples: &[f32]) -> HarnessResult<()> {
        for (i, &sample) in samples.iter().enumerate() {
            if sample < -1.0 || sample > 1.0 {
                return Err(HarnessError::ValidationError(format!(
                    "Audio sample {} out of range: {} (expected [-1.0, 1.0])",
                    i, sample
                )));
            }
        }
        Ok(())
    }

    /// Assert audio has minimum duration
    pub fn assert_audio_min_duration(
        &self,
        samples: &[f32],
        sample_rate: u32,
        min_duration_secs: f32,
    ) -> HarnessResult<()> {
        let duration = samples.len() as f32 / sample_rate as f32;
        if duration < min_duration_secs {
            return Err(HarnessError::ValidationError(format!(
                "Audio too short: {:.3}s (minimum: {:.3}s)",
                duration, min_duration_secs
            )));
        }
        Ok(())
    }

    /// Assert we received at least N audio packets
    pub fn assert_received_audio_packets(
        &self,
        client: &TestClient,
        min_packets: u32,
    ) -> HarnessResult<()> {
        let count = client.received_audio_packet_count();
        if count < min_packets {
            return Err(HarnessError::ValidationError(format!(
                "Expected at least {} audio packets, received {}",
                min_packets, count
            )));
        }
        Ok(())
    }

    // ========================================================================
    // Video Assertions
    // ========================================================================

    /// Assert video frame dimensions
    ///
    /// Validates that frame data size matches expected YUV420P dimensions
    pub fn assert_frame_dimensions(
        &self,
        frame: &[u8],
        width: u32,
        height: u32,
    ) -> HarnessResult<()> {
        let expected_size = (width * height) as usize          // Y plane
            + ((width * height) / 4) as usize * 2; // U + V planes (4:2:0)

        if frame.len() != expected_size {
            return Err(HarnessError::ValidationError(format!(
                "Frame size mismatch: {} bytes (expected {} for {}x{} YUV420P)",
                frame.len(),
                expected_size,
                width,
                height
            )));
        }

        Ok(())
    }

    /// Assert frame is not completely black
    pub fn assert_frame_not_black(
        &self,
        frame: &[u8],
        width: u32,
        height: u32,
    ) -> HarnessResult<()> {
        let y_size = (width * height) as usize;
        let y_plane = &frame[..y_size.min(frame.len())];

        // Calculate average luma
        let avg_luma: f32 = y_plane.iter().map(|&b| b as f32).sum::<f32>() / y_plane.len() as f32;

        // Black in YUV is Y=16
        if avg_luma < 20.0 {
            return Err(HarnessError::ValidationError(format!(
                "Frame is black: average luma = {:.1} (expected > 20)",
                avg_luma
            )));
        }

        Ok(())
    }

    /// Assert frame has expected average luma (brightness)
    pub fn assert_frame_brightness(
        &self,
        frame: &[u8],
        width: u32,
        height: u32,
        expected_luma: u8,
        tolerance: u8,
    ) -> HarnessResult<()> {
        let y_size = (width * height) as usize;
        let y_plane = &frame[..y_size.min(frame.len())];

        let avg_luma: f32 = y_plane.iter().map(|&b| b as f32).sum::<f32>() / y_plane.len() as f32;
        let avg_luma_u8 = avg_luma as u8;

        let diff = (avg_luma_u8 as i16 - expected_luma as i16).unsigned_abs() as u8;
        if diff > tolerance {
            return Err(HarnessError::ValidationError(format!(
                "Frame brightness mismatch: {} (expected {} +/- {})",
                avg_luma_u8, expected_luma, tolerance
            )));
        }

        Ok(())
    }

    /// Assert two frames are similar
    pub fn assert_frames_similar(&self, a: &[u8], b: &[u8], tolerance: f32) -> HarnessResult<()> {
        if a.len() != b.len() {
            return Err(HarnessError::ValidationError(format!(
                "Frame size mismatch: {} vs {} bytes",
                a.len(),
                b.len()
            )));
        }

        // Calculate normalized difference
        let diff_sum: f32 = a
            .iter()
            .zip(b.iter())
            .map(|(&x, &y)| (x as f32 - y as f32).powi(2))
            .sum();

        let mse = diff_sum / a.len() as f32;
        let normalized_diff = mse / (255.0 * 255.0); // Normalize to [0, 1]

        if normalized_diff > tolerance {
            return Err(HarnessError::ValidationError(format!(
                "Frames differ: normalized MSE = {:.4} (tolerance: {:.4})",
                normalized_diff, tolerance
            )));
        }

        Ok(())
    }

    /// Assert we received at least N video packets
    pub fn assert_received_video_packets(
        &self,
        client: &TestClient,
        min_packets: u32,
    ) -> HarnessResult<()> {
        let count = client.received_video_packet_count();
        if count < min_packets {
            return Err(HarnessError::ValidationError(format!(
                "Expected at least {} video packets, received {}",
                min_packets, count
            )));
        }
        Ok(())
    }

    // ========================================================================
    // Timing Assertions
    // ========================================================================

    /// Assert operation completed within time limit
    pub async fn assert_completes_within<F, T>(
        &self,
        timeout: Duration,
        operation: F,
    ) -> HarnessResult<T>
    where
        F: std::future::Future<Output = HarnessResult<T>>,
    {
        match tokio::time::timeout(timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(HarnessError::Timeout(format!(
                "Operation did not complete within {:?}",
                timeout
            ))),
        }
    }

    // ========================================================================
    // Connection Assertions
    // ========================================================================

    /// Assert client is connected
    pub async fn assert_connected(&self, client: &TestClient) -> HarnessResult<()> {
        if !client.is_connected().await {
            return Err(HarnessError::ValidationError(format!(
                "Client {} is not connected",
                client.peer_id()
            )));
        }
        Ok(())
    }

    /// Assert all clients are connected
    pub async fn assert_all_connected(&self, clients: &[&TestClient]) -> HarnessResult<()> {
        for client in clients {
            self.assert_connected(*client).await?;
        }
        Ok(())
    }

    // ========================================================================
    // Quality Metrics Assertions
    // ========================================================================

    /// Assert packet loss rate is below threshold
    ///
    /// # Arguments
    ///
    /// * `client` - Test client to check
    /// * `max_loss_rate` - Maximum acceptable loss rate (0.0 to 1.0)
    pub async fn assert_audio_loss_rate_below(
        &self,
        client: &TestClient,
        max_loss_rate: f32,
    ) -> HarnessResult<()> {
        let loss_rate = client.audio_packet_loss_rate().await;
        if loss_rate > max_loss_rate {
            return Err(HarnessError::ValidationError(format!(
                "Audio packet loss rate {:.2}% exceeds maximum {:.2}%",
                loss_rate * 100.0,
                max_loss_rate * 100.0
            )));
        }
        info!(
            "Audio loss rate {:.2}% is within acceptable range (max: {:.2}%)",
            loss_rate * 100.0,
            max_loss_rate * 100.0
        );
        Ok(())
    }

    /// Assert video packet loss rate is below threshold
    pub async fn assert_video_loss_rate_below(
        &self,
        client: &TestClient,
        max_loss_rate: f32,
    ) -> HarnessResult<()> {
        let loss_rate = client.video_packet_loss_rate().await;
        if loss_rate > max_loss_rate {
            return Err(HarnessError::ValidationError(format!(
                "Video packet loss rate {:.2}% exceeds maximum {:.2}%",
                loss_rate * 100.0,
                max_loss_rate * 100.0
            )));
        }
        info!(
            "Video loss rate {:.2}% is within acceptable range (max: {:.2}%)",
            loss_rate * 100.0,
            max_loss_rate * 100.0
        );
        Ok(())
    }

    /// Assert late packet count is below threshold
    ///
    /// # Arguments
    ///
    /// * `client` - Test client to check
    /// * `max_late_packets` - Maximum acceptable late packet count
    pub async fn assert_late_packets_below(
        &self,
        client: &TestClient,
        max_late_audio: u64,
        max_late_video: u64,
    ) -> HarnessResult<()> {
        let audio_late = client.audio_late_packets().await;
        let video_late = client.video_late_packets().await;

        if audio_late > max_late_audio {
            return Err(HarnessError::ValidationError(format!(
                "Audio late packets {} exceeds maximum {}",
                audio_late, max_late_audio
            )));
        }

        if video_late > max_late_video {
            return Err(HarnessError::ValidationError(format!(
                "Video late packets {} exceeds maximum {}",
                video_late, max_late_video
            )));
        }

        info!(
            "Late packets within range - audio: {}/{}, video: {}/{}",
            audio_late, max_late_audio, video_late, max_late_video
        );
        Ok(())
    }

    /// Assert connection time is established
    pub async fn assert_connection_established(
        &self,
        client: &TestClient,
    ) -> HarnessResult<()> {
        let metrics = client.get_quality_metrics().await;

        if metrics.connection_time_ms == 0 {
            return Err(HarnessError::ValidationError(
                "Connection time not tracked (still 0ms)".to_string(),
            ));
        }

        info!("Connection time: {}ms", metrics.connection_time_ms);
        Ok(())
    }

    /// Assert first packet latency is below threshold
    ///
    /// # Arguments
    ///
    /// * `client` - Test client to check
    /// * `max_latency_ms` - Maximum acceptable first packet latency in milliseconds
    pub async fn assert_first_packet_latency_below(
        &self,
        client: &TestClient,
        max_latency_ms: u64,
    ) -> HarnessResult<()> {
        let metrics = client.get_quality_metrics().await;

        match metrics.first_packet_latency_ms {
            Some(latency) => {
                if latency > max_latency_ms {
                    return Err(HarnessError::ValidationError(format!(
                        "First packet latency {}ms exceeds maximum {}ms",
                        latency, max_latency_ms
                    )));
                }
                info!(
                    "First packet latency {}ms is within acceptable range (max: {}ms)",
                    latency, max_latency_ms
                );
                Ok(())
            }
            None => Err(HarnessError::ValidationError(
                "No packets received - cannot measure first packet latency".to_string(),
            )),
        }
    }

    /// Assert jitter buffer health is good
    ///
    /// Checks that buffer statistics indicate healthy operation:
    /// - No buffer overruns
    /// - Dropped frames below threshold
    /// - Late packets below threshold
    pub async fn assert_jitter_buffer_healthy(
        &self,
        client: &TestClient,
        max_dropped_frames: u64,
        max_late_packets: u64,
    ) -> HarnessResult<()> {
        let audio_stats = client.get_audio_buffer_stats().await;
        let video_stats = client.get_video_buffer_stats().await;

        // Check audio buffer
        if audio_stats.buffer_overrun_count > 0 {
            return Err(HarnessError::ValidationError(format!(
                "Audio jitter buffer experienced {} overruns",
                audio_stats.buffer_overrun_count
            )));
        }

        if audio_stats.dropped_frames > max_dropped_frames {
            return Err(HarnessError::ValidationError(format!(
                "Audio dropped frames {} exceeds maximum {}",
                audio_stats.dropped_frames, max_dropped_frames
            )));
        }

        if audio_stats.late_packet_count > max_late_packets {
            return Err(HarnessError::ValidationError(format!(
                "Audio late packets {} exceeds maximum {}",
                audio_stats.late_packet_count, max_late_packets
            )));
        }

        // Check video buffer
        if video_stats.buffer_overrun_count > 0 {
            return Err(HarnessError::ValidationError(format!(
                "Video jitter buffer experienced {} overruns",
                video_stats.buffer_overrun_count
            )));
        }

        if video_stats.dropped_frames > max_dropped_frames {
            return Err(HarnessError::ValidationError(format!(
                "Video dropped frames {} exceeds maximum {}",
                video_stats.dropped_frames, max_dropped_frames
            )));
        }

        if video_stats.late_packet_count > max_late_packets {
            return Err(HarnessError::ValidationError(format!(
                "Video late packets {} exceeds maximum {}",
                video_stats.late_packet_count, max_late_packets
            )));
        }

        info!(
            "Jitter buffers healthy - audio: {} frames, {} dropped; video: {} frames, {} dropped",
            audio_stats.current_frames,
            audio_stats.dropped_frames,
            video_stats.current_frames,
            video_stats.dropped_frames
        );
        Ok(())
    }

    /// Assert quality metrics meet minimum thresholds
    ///
    /// Comprehensive check for overall connection quality
    pub async fn assert_quality_metrics(
        &self,
        client: &TestClient,
        max_loss_rate: f32,
        max_late_packets: u64,
        max_first_packet_latency_ms: Option<u64>,
    ) -> HarnessResult<()> {
        let metrics = client.get_quality_metrics().await;

        // Check connection time
        if metrics.connection_time_ms == 0 {
            return Err(HarnessError::ValidationError(
                "Connection time not tracked".to_string(),
            ));
        }

        // Check loss rates
        let audio_loss = client.audio_packet_loss_rate().await;
        let video_loss = client.video_packet_loss_rate().await;

        if audio_loss > max_loss_rate {
            return Err(HarnessError::ValidationError(format!(
                "Audio loss rate {:.2}% exceeds maximum {:.2}%",
                audio_loss * 100.0,
                max_loss_rate * 100.0
            )));
        }

        if video_loss > max_loss_rate {
            return Err(HarnessError::ValidationError(format!(
                "Video loss rate {:.2}% exceeds maximum {:.2}%",
                video_loss * 100.0,
                max_loss_rate * 100.0
            )));
        }

        // Check late packets
        let audio_late = client.audio_late_packets().await;
        let video_late = client.video_late_packets().await;

        if audio_late > max_late_packets || video_late > max_late_packets {
            return Err(HarnessError::ValidationError(format!(
                "Late packets exceed threshold: audio={}, video={}, max={}",
                audio_late, video_late, max_late_packets
            )));
        }

        // Check first packet latency if specified
        if let Some(max_latency) = max_first_packet_latency_ms {
            if let Some(latency) = metrics.first_packet_latency_ms {
                if latency > max_latency {
                    return Err(HarnessError::ValidationError(format!(
                        "First packet latency {}ms exceeds maximum {}ms",
                        latency, max_latency
                    )));
                }
            }
        }

        info!(
            "Quality metrics validated - connection: {}ms, audio: {:.2}% loss/{} late, video: {:.2}% loss/{} late",
            metrics.connection_time_ms,
            audio_loss * 100.0,
            audio_late,
            video_loss * 100.0,
            video_late
        );

        Ok(())
    }

    /// Get quality metrics summary for logging
    pub async fn quality_metrics_summary(&self, client: &TestClient) -> ClientQualityMetrics {
        client.get_quality_metrics().await
    }
}

impl Default for OutputValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_similar_identical() {
        let validator = OutputValidator::new();
        let samples = vec![0.1, 0.2, 0.3, 0.4, 0.5];

        let result = validator.assert_audio_similar(&samples, &samples, 0.0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_audio_similar_within_tolerance() {
        let validator = OutputValidator::new();
        let a = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let b = vec![0.11, 0.21, 0.31, 0.41, 0.51]; // Small differences

        let result = validator.assert_audio_similar(&a, &b, 0.1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_audio_similar_exceeds_tolerance() {
        let validator = OutputValidator::new();
        let a = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let b = vec![0.9, 0.8, 0.7, 0.6, 0.5]; // Large differences

        let result = validator.assert_audio_similar(&a, &b, 0.01);
        assert!(result.is_err());
    }

    #[test]
    fn test_audio_not_silent() {
        let validator = OutputValidator::new();

        // Audible signal
        let signal = vec![0.5, -0.5, 0.5, -0.5];
        assert!(validator.assert_audio_not_silent(&signal).is_ok());

        // Silent signal
        let silence = vec![0.0, 0.0, 0.0, 0.0];
        assert!(validator.assert_audio_not_silent(&silence).is_err());
    }

    #[test]
    fn test_frame_dimensions() {
        let validator = OutputValidator::new();

        // Correct size for 640x480 YUV420P
        let frame_size = 640 * 480 + (640 * 480 / 4) * 2;
        let frame = vec![128u8; frame_size];

        assert!(validator.assert_frame_dimensions(&frame, 640, 480).is_ok());

        // Wrong size
        let wrong_frame = vec![128u8; 1000];
        assert!(validator
            .assert_frame_dimensions(&wrong_frame, 640, 480)
            .is_err());
    }

    #[test]
    fn test_rms_calculation() {
        let validator = OutputValidator::new();

        // Full scale sine wave has RMS = 1/sqrt(2) ≈ 0.707
        let sine: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin()).collect();
        let rms = validator.calculate_rms(&sine);
        assert!((rms - 0.707).abs() < 0.1);

        // Silence has RMS = 0
        let silence = vec![0.0f32; 100];
        assert_eq!(validator.calculate_rms(&silence), 0.0);
    }
}
