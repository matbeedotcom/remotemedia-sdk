//! Audio capture commands

use crate::AppState;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use remotemedia_core::data::RuntimeData;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Start listening from the microphone
#[tauri::command]
pub async fn start_listening(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Check if pipeline session exists
    {
        let session_guard = state.session.lock().await;
        if session_guard.is_none() {
            return Err("Pipeline not initialized".to_string());
        }
    }

    // Check if already listening
    if state.audio_active.load(Ordering::SeqCst) {
        return Err("Already listening".to_string());
    }

    tracing::info!("Starting audio capture");

    // Set active flag before spawning thread
    state.audio_active.store(true, Ordering::SeqCst);

    // Get the audio sender channel
    let audio_tx = {
        let audio_tx_guard = state.audio_tx.lock().await;
        audio_tx_guard.clone().ok_or_else(|| "Audio channel not initialized".to_string())?
    };

    // Clone what we need for the audio thread
    let app_handle = app.clone();
    let audio_active = state.audio_active.clone();

    // Spawn audio capture in a dedicated thread (cpal::Stream is not Send)
    std::thread::spawn(move || {
        if let Err(e) = run_audio_capture(app_handle, audio_active.clone(), audio_tx) {
            tracing::error!("Audio capture error: {}", e);
            audio_active.store(false, Ordering::SeqCst);
        }
    });

    // Emit VAD state change
    app.emit(
        "vad_state",
        serde_json::json!({
            "active": true,
            "speaking": false
        }),
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn run_audio_capture(
    app: AppHandle,
    audio_active: Arc<std::sync::atomic::AtomicBool>,
    audio_tx: tokio::sync::mpsc::UnboundedSender<RuntimeData>,
) -> Result<(), String> {
    // Get the default input device
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No input device available".to_string())?;

    tracing::info!("Using input device: {}", device.name().unwrap_or_default());

    // Get the default input config
    let config = device
        .default_input_config()
        .map_err(|e| format!("Failed to get input config: {}", e))?;

    tracing::info!(
        "Audio config: {} Hz, {} channels, {:?}",
        config.sample_rate().0,
        config.channels(),
        config.sample_format()
    );

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    // Build the input stream
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_input_stream::<f32>(
            &device,
            &config.into(),
            app.clone(),
            sample_rate,
            channels,
            audio_tx,
        ),
        cpal::SampleFormat::I16 => build_input_stream::<i16>(
            &device,
            &config.into(),
            app.clone(),
            sample_rate,
            channels,
            audio_tx,
        ),
        cpal::SampleFormat::U16 => build_input_stream::<u16>(
            &device,
            &config.into(),
            app.clone(),
            sample_rate,
            channels,
            audio_tx,
        ),
        _ => return Err("Unsupported sample format".to_string()),
    }
    .map_err(|e| format!("Failed to build input stream: {}", e))?;

    // Start the stream
    stream
        .play()
        .map_err(|e| format!("Failed to start audio stream: {}", e))?;

    tracing::info!("Audio capture started");

    // Keep the stream alive while audio_active is true
    while audio_active.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    tracing::info!("Audio capture stopped");
    // Stream is dropped here, stopping capture
    Ok(())
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    app: AppHandle,
    sample_rate: u32,
    channels: u16,
    audio_tx: tokio::sync::mpsc::UnboundedSender<RuntimeData>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample + cpal::SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let mut sample_count: u64 = 0;

    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Convert samples to f32
            let samples: Vec<f32> = data.iter().map(|s| f32::from_sample(*s)).collect();

            // Calculate RMS for logging (every ~1 second)
            sample_count += samples.len() as u64;
            if sample_count % (sample_rate as u64) < samples.len() as u64 {
                let rms: f32 =
                    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
                tracing::debug!("Audio RMS: {:.4}, samples: {}", rms, samples.len());
            }

            // Create RuntimeData for the audio
            let audio_data = RuntimeData::Audio {
                samples: samples.clone(),
                sample_rate,
                channels: channels as u32,
                stream_id: None,
                timestamp_us: None,
                arrival_ts_us: None,
            };

            // Send to pipeline via channel
            if let Err(e) = audio_tx.send(audio_data) {
                tracing::warn!("Failed to send audio to pipeline: {}", e);
            }

            // Also emit to frontend for visualization (optional)
            let _ = app.emit(
                "audio_level",
                serde_json::json!({
                    "rms": (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt(),
                    "sample_count": samples.len(),
                }),
            );
        },
        move |err| {
            tracing::error!("Audio stream error: {}", err);
        },
        None,
    )
}

/// Stop listening from the microphone
#[tauri::command]
pub async fn stop_listening(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if !state.audio_active.load(Ordering::SeqCst) {
        return Ok(()); // Already stopped
    }

    tracing::info!("Stopping audio capture");

    // Signal the audio thread to stop
    state.audio_active.store(false, Ordering::SeqCst);

    // Emit VAD state change
    app.emit("vad_state", serde_json::json!({
        "active": false,
        "speaking": false
    }))
    .map_err(|e| e.to_string())?;

    Ok(())
}
