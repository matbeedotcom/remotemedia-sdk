//! tts-dataset-gen - Generate diverse TTS audio datasets for wake-word model training
//!
//! Takes text input, generates multiple audio samples with voice/speed variation
//! using TTS models, and saves WAV files to a dataset directory with a metadata CSV.
//!
//! # Usage
//!
//! ```bash
//! # Generate 5 samples of a phrase using Kokoro TTS
//! tts-dataset-gen --text "hey assistant" --samples 5 --output-dir ./dataset
//!
//! # Generate from a text file with multiple phrases
//! tts-dataset-gen --text-file phrases.txt --samples 3 --output-dir ./dataset
//!
//! # Use multiple models for more diversity
//! tts-dataset-gen --text "hello world" --samples 2 --models kokoro,voxtral
//! ```

// Force-link the Python nodes crate so inventory discovers the node providers
use remotemedia_python_nodes as _;

use anyhow::{Context, Result};
use clap::Parser;
use remotemedia_cli::pipeline;
use remotemedia_core::data::RuntimeData;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Pipeline YAML embedded at compile time
const PIPELINE_YAML: &str = include_str!("../../pipelines/tts-dataset.yaml");

/// Available Kokoro voices for diversity
const KOKORO_VOICES: &[&str] = &[
    "af_heart",
    "af_bella",
    "am_adam",
    "bf_emma",
    "bm_george",
];

/// Speed values for diversity
const SPEED_VALUES: &[f32] = &[0.9, 0.95, 1.0, 1.05, 1.1];

/// VibeVoice doesn't have named voices - diversity comes from speed variation
/// and the model's inherent variation per run
const VIBEVOICE_VARIANTS: &[&str] = &[
    "default_0",
    "default_1",
    "default_2",
    "default_3",
    "default_4",
];

/// CosyVoice3 modes for diversity (when no reference audio is available, use SFT)
const COSYVOICE3_VARIANTS: &[&str] = &[
    "variant_0",
    "variant_1",
    "variant_2",
    "variant_3",
    "variant_4",
];

/// Generate diverse TTS audio datasets for wake-word model training
#[derive(Parser)]
#[command(name = "tts-dataset-gen")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Text to synthesize (can be specified multiple times)
    #[arg(short = 't', long, action = clap::ArgAction::Append)]
    text: Vec<String>,

    /// File with one text per line
    #[arg(short = 'f', long)]
    text_file: Option<PathBuf>,

    /// Number of samples to generate per text
    #[arg(short = 's', long, default_value = "5")]
    samples: usize,

    /// Output directory for generated WAV files
    #[arg(short = 'o', long, default_value = "./tts-dataset")]
    output_dir: PathBuf,

    /// Comma-separated list of TTS models to use (kokoro, voxtral)
    #[arg(short = 'm', long, default_value = "kokoro")]
    models: String,

    /// Output sample rate in Hz
    #[arg(long, default_value = "24000")]
    sample_rate: u32,

    /// Per-synthesis timeout
    #[arg(long, default_value = "120s", value_parser = parse_duration)]
    timeout: Duration,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress non-error output
    #[arg(short, long)]
    quiet: bool,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.ends_with("ms") {
        s[..s.len() - 2]
            .parse()
            .map(Duration::from_millis)
            .map_err(|_| "Invalid ms".into())
    } else if s.ends_with('s') {
        s[..s.len() - 1]
            .parse()
            .map(Duration::from_secs)
            .map_err(|_| "Invalid s".into())
    } else if s.ends_with('m') {
        s[..s.len() - 1]
            .parse::<u64>()
            .map(|m| Duration::from_secs(m * 60))
            .map_err(|_| "Invalid m".into())
    } else {
        s.parse()
            .map(Duration::from_secs)
            .map_err(|_| "Invalid duration".into())
    }
}

/// A single generation task
struct GenerationTask {
    text: String,
    model: String,
    voice: String,
    speed: f32,
    speaker_desc: String,
    model_path: String,
    index: usize,
}

/// Metadata row for CSV output
struct MetadataRow {
    filename: String,
    text: String,
    model: String,
    voice: String,
    speed: f32,
    sample_rate: u32,
    duration_s: f32,
}

/// Substitute template variables in pipeline YAML
fn substitute_pipeline_vars(yaml: &str, task: &GenerationTask, sample_rate: u32) -> String {
    let node_type = match task.model.as_str() {
        "kokoro" => "KokoroTTSNode",
        "vibevoice" => "VibeVoiceTTSNode",
        "cosyvoice3" => "CosyVoice3TTSNode",
        "voxtral" => "VoxtralTTSNode",
        other => other, // Allow raw node type names
    };

    yaml.replace("${TTS_NODE_TYPE:-KokoroTTSNode}", node_type)
        .replace("${VOICE:-af_heart}", &task.voice)
        .replace("${SPEED:-1.0}", &format!("{:.2}", task.speed))
        .replace("${SAMPLE_RATE:-24000}", &sample_rate.to_string())
        .replace("${LANG_CODE:-a}", "a")
        .replace(
            "${SPEAKER_DESC:-A clear, neutral voice speaks at a moderate pace.}",
            &task.speaker_desc,
        )
        .replace("${MODEL_PATH:-microsoft/VibeVoice-1.5B}", &task.model_path)
}

/// Generate a sanitized filename from task parameters
fn generate_filename(task: &GenerationTask) -> String {
    // Create a short hash of the text for the filename
    let text_hash: u32 = task
        .text
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));

    let voice_clean = task.voice.replace(['/', '\\', ' '], "_");
    let speed_str = format!("{:.0}", task.speed * 100.0);

    format!(
        "{}_{}_spd{}_{:08x}_{:04}.wav",
        task.model, voice_clean, speed_str, text_hash, task.index
    )
}

/// Write audio samples to a WAV file
fn write_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).context("Failed to create WAV writer")?;
    for &sample in samples {
        let s16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
        writer.write_sample(s16)?;
    }
    writer.finalize().context("Failed to finalize WAV file")?;
    Ok(())
}

/// Write metadata CSV
fn write_metadata_csv(path: &std::path::Path, rows: &[MetadataRow]) -> Result<()> {
    let mut file = std::fs::File::create(path).context("Failed to create metadata.csv")?;
    writeln!(file, "filename,text,model,voice,speed,sample_rate,duration_s")?;
    for row in rows {
        // Escape text field for CSV (double-quote any quotes, wrap in quotes)
        let escaped_text = row.text.replace('"', "\"\"");
        writeln!(
            file,
            "{},\"{}\",{},{},{:.2},{},{:.3}",
            row.filename, escaped_text, row.model, row.voice, row.speed, row.sample_rate,
            row.duration_s
        )?;
    }
    Ok(())
}

/// Build the list of generation tasks from inputs
fn build_tasks(texts: &[String], models: &[String], samples_per_text: usize) -> Vec<GenerationTask> {
    let mut tasks = Vec::new();
    let mut global_index = 0;

    for text in texts {
        for model in models {
            for sample_idx in 0..samples_per_text {
                let (voice, speed, speaker_desc, model_path) = match model.as_str() {
                    "kokoro" => {
                        let voice = KOKORO_VOICES[sample_idx % KOKORO_VOICES.len()].to_string();
                        let speed = SPEED_VALUES[sample_idx % SPEED_VALUES.len()];
                        (voice, speed, String::new(), String::new())
                    }
                    "vibevoice" => {
                        let variant = VIBEVOICE_VARIANTS[sample_idx % VIBEVOICE_VARIANTS.len()].to_string();
                        let speed = SPEED_VALUES[sample_idx % SPEED_VALUES.len()];
                        (variant, speed, String::new(), "microsoft/VibeVoice-Realtime-0.5B".to_string())
                    }
                    "cosyvoice3" => {
                        let variant = COSYVOICE3_VARIANTS[sample_idx % COSYVOICE3_VARIANTS.len()].to_string();
                        let speed = SPEED_VALUES[sample_idx % SPEED_VALUES.len()];
                        (variant, speed, String::new(), "pretrained_models/Fun-CosyVoice3-0.5B".to_string())
                    }
                    _ => {
                        let speed = SPEED_VALUES[sample_idx % SPEED_VALUES.len()];
                        ("default".to_string(), speed, String::new(), String::new())
                    }
                };

                tasks.push(GenerationTask {
                    text: text.clone(),
                    model: model.clone(),
                    voice,
                    speed,
                    speaker_desc,
                    model_path,
                    index: global_index,
                });
                global_index += 1;
            }
        }
    }

    tasks
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging to stderr
    let filter = if args.quiet {
        "error"
    } else {
        match args.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    };
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .init();

    // Collect text inputs
    let mut texts = args.text.clone();
    if let Some(ref text_file) = args.text_file {
        let content = std::fs::read_to_string(text_file)
            .with_context(|| format!("Failed to read text file: {}", text_file.display()))?;
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                texts.push(line.to_string());
            }
        }
    }

    if texts.is_empty() {
        anyhow::bail!("No text input provided. Use --text or --text-file");
    }

    // Parse model list
    let models: Vec<String> = args
        .models
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    if models.is_empty() {
        anyhow::bail!("No models specified");
    }

    // Create output directory
    std::fs::create_dir_all(&args.output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            args.output_dir.display()
        )
    })?;

    // Build generation tasks
    let tasks = build_tasks(&texts, &models, args.samples);
    let total_tasks = tasks.len();

    if !args.quiet {
        eprintln!(
            "Generating {} samples ({} texts x {} models x {} samples/text)",
            total_tasks,
            texts.len(),
            models.len(),
            args.samples
        );
        eprintln!("Output directory: {}", args.output_dir.display());
    }

    // Create pipeline runner (reuse across all tasks)
    let runner = pipeline::create_runner()?;
    let mut metadata_rows: Vec<MetadataRow> = Vec::new();
    let mut success_count = 0;
    let mut error_count = 0;

    // Process each task
    for (task_idx, task) in tasks.iter().enumerate() {
        if !args.quiet {
            eprintln!(
                "[{}/{}] model={} voice={} speed={:.2} text=\"{}\"",
                task_idx + 1,
                total_tasks,
                task.model,
                task.voice,
                task.speed,
                if task.text.len() > 50 {
                    format!("{}...", &task.text[..50])
                } else {
                    task.text.clone()
                }
            );
        }

        // Substitute template variables and parse manifest
        let manifest_yaml = substitute_pipeline_vars(PIPELINE_YAML, task, args.sample_rate);
        let manifest = match pipeline::parse_manifest(&manifest_yaml) {
            Ok(m) => Arc::new(m),
            Err(e) => {
                tracing::error!("Failed to parse manifest for task {}: {}", task_idx, e);
                error_count += 1;
                continue;
            }
        };

        // Create streaming session and send text
        let input_data = RuntimeData::Text(task.text.clone());

        // Use streaming session to collect all audio chunks from multi-output TTS node
        let result = tokio::time::timeout(args.timeout, async {
            let mut session = pipeline::StreamingSession::new(&runner, manifest).await?;
            session.send(input_data).await?;

            // Signal no more input - this lets the session router shut down
            // after processing is complete, which closes the output channel
            session.signal_input_complete();

            // Collect all audio chunks until the session ends (recv returns None)
            let mut all_samples: Vec<f32> = Vec::new();
            let mut chunk_sample_rate = args.sample_rate;

            while let Some(output) = session.recv().await? {
                match output {
                    RuntimeData::Audio {
                        samples,
                        sample_rate,
                        ..
                    } => {
                        chunk_sample_rate = sample_rate;
                        all_samples.extend_from_slice(&samples);
                    }
                    _ => {
                        tracing::debug!("Received non-audio output, skipping");
                    }
                }
            }

            Ok::<(Vec<f32>, u32), anyhow::Error>((all_samples, chunk_sample_rate))
        })
        .await;

        match result {
            Ok(Ok((samples, actual_sample_rate))) if !samples.is_empty() => {
                let filename = generate_filename(task);
                let output_path = args.output_dir.join(&filename);
                let duration_s = samples.len() as f32 / actual_sample_rate as f32;

                match write_wav(&output_path, &samples, actual_sample_rate) {
                    Ok(()) => {
                        tracing::info!(
                            "Wrote {} ({:.2}s, {} samples)",
                            filename,
                            duration_s,
                            samples.len()
                        );
                        metadata_rows.push(MetadataRow {
                            filename,
                            text: task.text.clone(),
                            model: task.model.clone(),
                            voice: task.voice.clone(),
                            speed: task.speed,
                            sample_rate: actual_sample_rate,
                            duration_s,
                        });
                        success_count += 1;
                    }
                    Err(e) => {
                        tracing::error!("Failed to write WAV {}: {}", filename, e);
                        error_count += 1;
                    }
                }
            }
            Ok(Ok(_)) => {
                tracing::warn!("Task {} produced no audio samples", task_idx);
                error_count += 1;
            }
            Ok(Err(e)) => {
                tracing::error!("Task {} failed: {}", task_idx, e);
                error_count += 1;
            }
            Err(_) => {
                tracing::error!("Task {} timed out after {:?}", task_idx, args.timeout);
                error_count += 1;
            }
        }
    }

    // Write metadata CSV
    let csv_path = args.output_dir.join("metadata.csv");
    write_metadata_csv(&csv_path, &metadata_rows)?;

    // Summary
    if !args.quiet {
        let total_duration: f32 = metadata_rows.iter().map(|r| r.duration_s).sum();
        eprintln!();
        eprintln!("--- Dataset Generation Summary ---");
        eprintln!("Output directory: {}", args.output_dir.display());
        eprintln!("Successful: {}/{}", success_count, total_tasks);
        if error_count > 0 {
            eprintln!("Failed: {}", error_count);
        }
        eprintln!("Total audio: {:.1}s", total_duration);
        eprintln!("Metadata: {}", csv_path.display());
    }

    if error_count > 0 && success_count == 0 {
        anyhow::bail!("All {} generation tasks failed", total_tasks);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_generate_filename() {
        let task = GenerationTask {
            text: "hello world".to_string(),
            model: "kokoro".to_string(),
            voice: "af_heart".to_string(),
            speed: 1.0,
            speaker_desc: String::new(),
            model_path: String::new(),
            index: 0,
        };
        let filename = generate_filename(&task);
        assert!(filename.starts_with("kokoro_af_heart_spd100_"));
        assert!(filename.ends_with("_0000.wav"));
    }

    #[test]
    fn test_generate_filename_vibevoice() {
        let task = GenerationTask {
            text: "test phrase".to_string(),
            model: "vibevoice".to_string(),
            voice: "default_0".to_string(),
            speed: 0.9,
            speaker_desc: String::new(),
            model_path: "microsoft/VibeVoice-Realtime-0.5B".to_string(),
            index: 42,
        };
        let filename = generate_filename(&task);
        assert!(filename.starts_with("vibevoice_default_0_spd90_"));
        assert!(filename.ends_with("_0042.wav"));
    }

    #[test]
    fn test_substitute_pipeline_vars() {
        let task = GenerationTask {
            text: "test".to_string(),
            model: "kokoro".to_string(),
            voice: "af_bella".to_string(),
            speed: 1.1,
            speaker_desc: String::new(),
            model_path: String::new(),
            index: 0,
        };
        let result = substitute_pipeline_vars(PIPELINE_YAML, &task, 16000);
        assert!(result.contains("KokoroTTSNode"));
        assert!(result.contains("af_bella"));
        assert!(result.contains("1.10"));
        assert!(result.contains("16000"));
    }

    #[test]
    fn test_substitute_pipeline_vars_vibevoice() {
        let task = GenerationTask {
            text: "test".to_string(),
            model: "vibevoice".to_string(),
            voice: "default_0".to_string(),
            speed: 0.95,
            speaker_desc: String::new(),
            model_path: "microsoft/VibeVoice-Realtime-0.5B".to_string(),
            index: 0,
        };
        let result = substitute_pipeline_vars(PIPELINE_YAML, &task, 24000);
        assert!(result.contains("VibeVoiceTTSNode"));
        assert!(result.contains("microsoft/VibeVoice-Realtime-0.5B"));
    }

    #[test]
    fn test_substitute_pipeline_vars_cosyvoice3() {
        let task = GenerationTask {
            text: "test".to_string(),
            model: "cosyvoice3".to_string(),
            voice: "variant_0".to_string(),
            speed: 1.0,
            speaker_desc: String::new(),
            model_path: "pretrained_models/Fun-CosyVoice3-0.5B".to_string(),
            index: 0,
        };
        let result = substitute_pipeline_vars(PIPELINE_YAML, &task, 24000);
        assert!(result.contains("CosyVoice3TTSNode"));
    }

    #[test]
    fn test_build_tasks() {
        let texts = vec!["hello".to_string(), "world".to_string()];
        let models = vec!["kokoro".to_string()];
        let tasks = build_tasks(&texts, &models, 3);

        // 2 texts * 1 model * 3 samples = 6 tasks
        assert_eq!(tasks.len(), 6);

        // Check voice cycling
        assert_eq!(tasks[0].voice, "af_heart");
        assert_eq!(tasks[1].voice, "af_bella");
        assert_eq!(tasks[2].voice, "am_adam");

        // Check speed cycling
        assert!((tasks[0].speed - 0.9).abs() < 0.01);
        assert!((tasks[1].speed - 0.95).abs() < 0.01);
        assert!((tasks[2].speed - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_build_tasks_multi_model() {
        let texts = vec!["test".to_string()];
        let models = vec!["kokoro".to_string(), "vibevoice".to_string()];
        let tasks = build_tasks(&texts, &models, 2);

        // 1 text * 2 models * 2 samples = 4 tasks
        assert_eq!(tasks.len(), 4);
        assert_eq!(tasks[0].model, "kokoro");
        assert_eq!(tasks[1].model, "kokoro");
        assert_eq!(tasks[2].model, "vibevoice");
        assert_eq!(tasks[3].model, "vibevoice");
    }

    #[test]
    fn test_write_wav() {
        let dir = std::env::temp_dir().join("tts_dataset_gen_test_wav");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.wav");

        // Generate a simple sine wave
        let sample_rate = 24000;
        let duration = 0.1; // 100ms
        let num_samples = (sample_rate as f32 * duration) as usize;
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin())
            .collect();

        write_wav(&path, &samples, sample_rate).unwrap();

        // Verify the file exists and has valid WAV content
        assert!(path.exists());
        let reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, sample_rate);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(reader.len() as usize, num_samples);

        // Cleanup
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_write_metadata_csv() {
        let dir = std::env::temp_dir().join("tts_dataset_gen_test_csv");
        std::fs::create_dir_all(&dir).unwrap();
        let csv_path = dir.join("metadata.csv");

        let rows = vec![
            MetadataRow {
                filename: "test_001.wav".to_string(),
                text: "hello world".to_string(),
                model: "kokoro".to_string(),
                voice: "af_heart".to_string(),
                speed: 1.0,
                sample_rate: 24000,
                duration_s: 1.5,
            },
            MetadataRow {
                filename: "test_002.wav".to_string(),
                text: "test with \"quotes\"".to_string(),
                model: "vibevoice".to_string(),
                voice: "speaker_0".to_string(),
                speed: 0.9,
                sample_rate: 24000,
                duration_s: 2.0,
            },
        ];

        write_metadata_csv(&csv_path, &rows).unwrap();

        let content = std::fs::read_to_string(&csv_path).unwrap();
        assert!(content.starts_with("filename,text,model,voice,speed,sample_rate,duration_s\n"));
        assert!(content.contains("test_001.wav"));
        assert!(content.contains("test_002.wav"));
        assert!(content.contains("kokoro"));
        assert!(content.contains("vibevoice"));

        // Cleanup
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("120s").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("60").unwrap(), Duration::from_secs(60));
    }
}
