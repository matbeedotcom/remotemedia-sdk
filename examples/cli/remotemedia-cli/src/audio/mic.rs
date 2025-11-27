//! Microphone capture using cpal

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use tokio::sync::broadcast;

/// Audio capture configuration
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size_ms: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 1,
            buffer_size_ms: 20,
        }
    }
}

/// Audio capture handle
pub struct AudioCapture {
    #[allow(dead_code)]
    stream: cpal::Stream,
    receiver: broadcast::Receiver<Vec<f32>>,
}

impl AudioCapture {
    /// Start capturing from the default input device
    pub fn start(config: CaptureConfig) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

        let supported_config = device
            .supported_input_configs()?
            .find(|c| c.channels() == config.channels)
            .ok_or_else(|| anyhow::anyhow!("No suitable input config"))?
            .with_sample_rate(cpal::SampleRate(config.sample_rate));

        let (tx, rx) = broadcast::channel(100);

        let stream = device.build_input_stream(
            &supported_config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(data.to_vec());
            },
            |err| {
                eprintln!("Audio capture error: {}", err);
            },
            None,
        )?;

        stream.play()?;

        Ok(Self {
            stream,
            receiver: rx,
        })
    }

    /// Receive the next audio buffer
    pub async fn recv(&mut self) -> Option<Vec<f32>> {
        self.receiver.recv().await.ok()
    }
}

/// Simple synchronous capture for one-shot recording
pub fn capture_sync(config: CaptureConfig, duration_ms: u32) -> Result<Vec<f32>> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

    let supported_config = device
        .supported_input_configs()?
        .find(|c| c.channels() == config.channels)
        .ok_or_else(|| anyhow::anyhow!("No suitable input config"))?
        .with_sample_rate(cpal::SampleRate(config.sample_rate));

    let (tx, rx) = mpsc::channel();
    let samples_needed = (config.sample_rate * duration_ms / 1000) as usize;
    let mut collected = Vec::with_capacity(samples_needed);

    let stream = device.build_input_stream(
        &supported_config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let _ = tx.send(data.to_vec());
        },
        |err| {
            eprintln!("Audio capture error: {}", err);
        },
        None,
    )?;

    stream.play()?;

    while collected.len() < samples_needed {
        if let Ok(data) = rx.recv_timeout(std::time::Duration::from_secs(1)) {
            collected.extend(data);
        } else {
            break;
        }
    }

    collected.truncate(samples_needed);
    Ok(collected)
}
