//! Speaker playback using cpal

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

/// Audio playback configuration
#[derive(Debug, Clone)]
pub struct PlaybackConfig {
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 1,
        }
    }
}

/// Audio playback handle
pub struct AudioPlayback {
    #[allow(dead_code)]
    stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    position: Arc<Mutex<usize>>,
}

impl AudioPlayback {
    /// Start playback on the default output device
    pub fn start(config: PlaybackConfig) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No output device available"))?;

        let supported_config = device
            .supported_output_configs()?
            .find(|c| c.channels() == config.channels)
            .ok_or_else(|| anyhow::anyhow!("No suitable output config"))?
            .with_sample_rate(cpal::SampleRate(config.sample_rate));

        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let position = Arc::new(Mutex::new(0usize));

        let buffer_clone = buffer.clone();
        let position_clone = position.clone();

        let stream = device.build_output_stream(
            &supported_config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let buf = buffer_clone.lock().unwrap();
                let mut pos = position_clone.lock().unwrap();

                for sample in data.iter_mut() {
                    if *pos < buf.len() {
                        *sample = buf[*pos];
                        *pos += 1;
                    } else {
                        *sample = 0.0;
                    }
                }
            },
            |err| {
                eprintln!("Audio playback error: {}", err);
            },
            None,
        )?;

        stream.play()?;

        Ok(Self {
            stream,
            buffer,
            position,
        })
    }

    /// Queue audio samples for playback
    pub fn queue(&self, samples: &[f32]) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.extend_from_slice(samples);
    }

    /// Clear the playback buffer
    pub fn clear(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        let mut position = self.position.lock().unwrap();
        buffer.clear();
        *position = 0;
    }

    /// Check if playback is complete
    pub fn is_complete(&self) -> bool {
        let buffer = self.buffer.lock().unwrap();
        let position = self.position.lock().unwrap();
        *position >= buffer.len()
    }
}

/// Play audio samples synchronously
pub fn play_sync(samples: &[f32], config: PlaybackConfig) -> Result<()> {
    let playback = AudioPlayback::start(config)?;
    playback.queue(samples);

    // Wait for playback to complete
    while !playback.is_complete() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Ok(())
}
