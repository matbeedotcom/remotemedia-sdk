//! emotion-activation-gen - Generate emotion-labelled prompts for activation extraction
//!
//! Generates dialogue scenarios with explicit emotion labels, suitable for feeding
//! into an LLM to capture residual-stream activations for emotion vector extraction.
//!
//! # Usage
//!
//! ```bash
//! # Generate 50 happy/neutral prompts for activation capture
//! emotion-activation-gen --emotions happy,neutral --samples 25 --output-dir ./prompts
//!
//! # Generate deflection prompts (real vs displayed emotion)
//! emotion-activation-gen --mode deflection --emotions happy,sad,angry --samples 10
//!
//! # Generate from a specific topic list
//! emotion-activation-gen --topic-file topics.txt --emotions happy,sad --samples 5
//! ```

use anyhow::{Context, Result};
use clap::Parser;
use rand::seq::SliceRandom;
use rand::thread_rng;
use rand::rngs::ThreadRng;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Emotion categories used in prompt generation
const EMOTIONS: &[&str] = &[
    "happy",
    "sad",
    "angry",
    "fearful",
    "surprised",
    "disgusted",
    "neutral",
];

/// Common names for dialogue participants
const NAMES: &[&str] = &[
    "Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Henry",
    "Iris", "Jack", "Karen", "Leo", "Mia", "Noah", "Olivia", "Peter",
    "Quinn", "Rachel", "Sam", "Tina", "Uma", "Victor", "Wendy", "Xander",
];

/// Conversation topics for prompt generation
const TOPICS: &[&str] = &[
    "a recent vacation",
    "work project deadline",
    "family reunion plans",
    "a funny incident at the grocery store",
    "trying a new restaurant",
    "weekend hiking trip",
    "book club discussion",
    "home renovation project",
    "learning to play guitar",
    "a surprise birthday party",
    "moving to a new city",
    "starting a new job",
    "cooking a complicated recipe",
    "watching a movie together",
    "planning a road trip",
    "a childhood memory",
    "gardening tips",
    "a recent concert",
    "pet adoption",
    "volunteering at the shelter",
];

/// Unexpressed emotion conversation topics (neutral topics to discuss while hiding emotion)
const NEUTRAL_TOPICS: &[&str] = &[
    "the weather forecast",
    "upcoming holiday plans",
    "a new TV show",
    "local restaurant recommendations",
    "sports scores",
    "grocery shopping list",
    "commute routes",
    "book recommendations",
    "home maintenance tips",
    "technology gadgets",
];

/// Prompt generation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
enum GenerationMode {
    /// Natural emotion expression (emotion matches dialogue)
    Natural,
    /// Emotion deflection (real emotion hidden, displayed emotion shown)
    Deflection,
    /// Unexpressed emotion on neutral topic
    UnexpressedNeutral,
    /// Unexpressed emotion in story writing
    UnexpressedStory,
    /// Unexpressed emotion while discussing others
    UnexpressedOthers,
}

/// A single generated prompt with emotion label
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionPrompt {
    /// Unique identifier
    pub id: usize,
    /// Generation mode
    pub mode: String,
    /// Real emotion (always present)
    pub real_emotion: String,
    /// Displayed emotion (for deflection modes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayed_emotion: Option<String>,
    /// Story emotion (for story writing mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_emotion: Option<String>,
    /// Other person's emotion (for discussing others mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_emotion: Option<String>,
    /// Topic/context
    pub topic: String,
    /// Conversation topic (for unexpressed modes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_topic: Option<String>,
    /// Participant names
    pub name_a: String,
    pub name_b: String,
    /// The generated prompt text (scenario + dialogue template)
    pub prompt: String,
    /// System prompt for the LLM
    pub system_prompt: String,
}

/// Generate emotion-labelled prompts for activation extraction
#[derive(Parser)]
#[command(name = "emotion-activation-gen")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Emotions to generate prompts for (comma-separated)
    #[arg(
        short = 'e',
        long,
        default_value = "happy,sad,angry,fearful,neutral",
        value_delimiter = ','
    )]
    emotions: Vec<String>,

    /// Number of samples per emotion
    #[arg(short = 's', long, default_value = "20")]
    samples: usize,

    /// Output directory for generated prompts
    #[arg(short = 'o', long, default_value = "./emotion-prompts")]
    output_dir: PathBuf,

    /// Generation mode
    #[arg(short = 'm', long, value_enum, default_value = "natural")]
    mode: GenerationMode,

    /// File with one topic per line (overrides built-in topics)
    #[arg(short = 't', long)]
    topic_file: Option<PathBuf>,

    /// LLM model for generation (informational, not used for LLM calls)
    #[arg(long, default_value = "meta-llama/Llama-3.1-8B-Instruct")]
    model: String,

    /// Target layer for activation capture (informational)
    #[arg(long, default_value = "21")]
    layer: usize,

    /// Hidden size of the model (informational)
    #[arg(long, default_value = "4096")]
    hidden_size: usize,

    /// Output format: jsonl, json, or yaml
    #[arg(long, default_value = "jsonl")]
    format: String,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress non-error output
    #[arg(short, long)]
    quiet: bool,
}

/// Load topics from file or use built-in list
fn load_topics(args: &Args) -> Result<Vec<String>> {
    if let Some(ref path) = args.topic_file {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read topic file: {}", path.display()))?;
        Ok(content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect())
    } else {
        Ok(match args.mode {
            GenerationMode::UnexpressedNeutral => NEUTRAL_TOPICS.iter().map(|s| s.to_string()).collect(),
            _ => TOPICS.iter().map(|s| s.to_string()).collect(),
        })
    }
}

/// Generate a random name pair
fn random_names(rng: &mut ThreadRng) -> (String, String) {
    let name_a = NAMES.choose(rng).unwrap().to_string();
    let mut name_b = NAMES.choose(rng).unwrap().to_string();
    while name_b == name_a {
        name_b = NAMES.choose(rng).unwrap().to_string();
    }
    (name_a, name_b)
}

/// Generate a natural emotion prompt
fn generate_natural_prompt(
    _rng: &mut ThreadRng,
    emotion: &str,
    topic: &str,
    name_a: &str,
    name_b: &str,
) -> EmotionPrompt {
    let prompt = format!(
        "Scenario: {} feels {} about {}. Include context for why they feel this way.\n\n\
         {}: [utterance]\n\n\
         {}: [response]\n\n\
         ...\n\n\
         Requirements:\n\
         1. {}'s dialogue should naturally reflect {} emotion\n\
         2. Keep it natural and grounded\n\
         3. Dialogue length is organic - 1-2 turns is enough\n\
         4. Either {} or {} may speak first",
        name_a, emotion, topic, name_a, name_b, name_a, emotion, name_a, name_b
    );

    let system_prompt = format!(
        "Generate a natural dialogue between {} and {} about {}. \
         {} should express {} emotion naturally through their words.",
        name_a, name_b, topic, name_a, emotion
    );

    EmotionPrompt {
        id: 0,
        mode: "natural".to_string(),
        real_emotion: emotion.to_string(),
        displayed_emotion: None,
        story_emotion: None,
        other_emotion: None,
        topic: topic.to_string(),
        conversation_topic: None,
        name_a: name_a.to_string(),
        name_b: name_b.to_string(),
        prompt,
        system_prompt,
    }
}

/// Generate an emotion deflection prompt
fn generate_deflection_prompt(
    rng: &mut ThreadRng,
    real_emotion: &str,
    topic: &str,
    name_a: &str,
    name_b: &str,
) -> EmotionPrompt {
    // Pick a different displayed emotion
    let displayed_emotion = loop {
        let candidate = EMOTIONS.choose(rng).unwrap();
        if candidate != &real_emotion && candidate != &"neutral" {
            break candidate.to_string();
        }
    };

    let prompt = format!(
        "Scenario: Describe where {} genuinely feels {} but appears {} about {}. \
         Must explicitly state {}'s real emotion. Include context for why they want to conceal.\n\n\
         {}: [utterance]\n\n\
         {}: [response]\n\n\
         ...\n\n\
         Requirements:\n\
         1. {}'s words should fully reflect {} emotion with no hints of {}\n\
         2. The hidden emotion exists only in the scenario\n\
         3. Keep it natural and grounded\n\
         4. Dialogue length is organic - 1-2 turns is enough",
        name_a, real_emotion, displayed_emotion, topic, name_a,
        name_a, name_b, name_a, displayed_emotion, real_emotion
    );

    let system_prompt = format!(
        "Generate a dialogue where {} hides their {} emotion and appears {} instead. \
         The topic is {}. {}'s real emotion should only be in the scenario description.",
        name_a, real_emotion, displayed_emotion, topic, name_a
    );

    EmotionPrompt {
        id: 0,
        mode: "deflection".to_string(),
        real_emotion: real_emotion.to_string(),
        displayed_emotion: Some(displayed_emotion),
        story_emotion: None,
        other_emotion: None,
        topic: topic.to_string(),
        conversation_topic: None,
        name_a: name_a.to_string(),
        name_b: name_b.to_string(),
        prompt,
        system_prompt,
    }
}

/// Generate an unexpressed emotion (neutral topic) prompt
fn generate_unexpressed_neutral_prompt(
    rng: &mut ThreadRng,
    emotion: &str,
    topic: &str,
    name_a: &str,
    name_b: &str,
) -> EmotionPrompt {
    let conversation_topic = NEUTRAL_TOPICS.choose(rng).unwrap().to_string();

    let prompt = format!(
        "Scenario context: {}\n\
         The topic {} will ask about: {}\n\n\
         Generate a brief scenario (2-4 sentences) where {} genuinely feels {}, \
         ending with their friend {} asking about a different topic.\n\n\
         Requirements:\n\
         1. Describe a situation related to \"{}\" that makes {} feel {}\n\
         2. Explicitly state that {} feels {}\n\
         3. End with {} asking {} about the conversation topic\n\
         4. Keep it concise - just the scenario description, no dialogue",
        topic, name_b, conversation_topic, name_a, emotion, name_b,
        topic, name_a, emotion, name_a, emotion, name_b, name_a
    );

    let system_prompt = format!(
        "Generate a scenario where {} feels {} about {} but the conversation \
         shifts to {}. The real emotion should only be in the scenario context.",
        name_a, emotion, topic, conversation_topic
    );

    EmotionPrompt {
        id: 0,
        mode: "unexpressed-neutral".to_string(),
        real_emotion: emotion.to_string(),
        displayed_emotion: None,
        story_emotion: None,
        other_emotion: None,
        topic: topic.to_string(),
        conversation_topic: Some(conversation_topic),
        name_a: name_a.to_string(),
        name_b: name_b.to_string(),
        prompt,
        system_prompt,
    }
}

/// Generate an unexpressed emotion (story writing) prompt
fn generate_unexpressed_story_prompt(
    rng: &mut ThreadRng,
    real_emotion: &str,
    topic: &str,
    name_a: &str,
) -> EmotionPrompt {
    // Pick a different story emotion
    let story_emotion = loop {
        let candidate = EMOTIONS.choose(rng).unwrap();
        if candidate != &real_emotion && candidate != &"neutral" {
            break candidate.to_string();
        }
    };

    let prompt = format!(
        "Scenario: {} is feeling {} about {}. They write/tell a story.\n\n\
         {}: [The story goes here, featuring characters who show {}...]\n\n\
         Requirements:\n\
         1. The scenario must explicitly state {}'s {} emotional state\n\
         2. After the scenario, {} writes/tells the story\n\
         3. The story should have characters clearly showing {}\n\
         4. The story's emotion ({}) is different from {}'s real emotion ({})\n\
         5. Keep the story grounded and natural",
        name_a, real_emotion, topic, name_a, story_emotion,
        name_a, real_emotion, name_a, story_emotion,
        story_emotion, name_a, real_emotion
    );

    let system_prompt = format!(
        "Generate a scenario where {} feels {} about {}, then write a story \
         with characters showing {} emotion. The story emotion differs from {}'s real emotion.",
        name_a, real_emotion, topic, story_emotion, name_a
    );

    EmotionPrompt {
        id: 0,
        mode: "unexpressed-story".to_string(),
        real_emotion: real_emotion.to_string(),
        displayed_emotion: None,
        story_emotion: Some(story_emotion),
        other_emotion: None,
        topic: topic.to_string(),
        conversation_topic: None,
        name_a: name_a.to_string(),
        name_b: "narrator".to_string(),
        prompt,
        system_prompt,
    }
}

/// Generate an unexpressed emotion (discussing others) prompt
fn generate_unexpressed_others_prompt(
    rng: &mut ThreadRng,
    real_emotion: &str,
    topic: &str,
    name_a: &str,
    name_b: &str,
) -> EmotionPrompt {
    // Pick another person's emotion
    let other_emotion = loop {
        let candidate = EMOTIONS.choose(rng).unwrap();
        if candidate != &real_emotion {
            break candidate.to_string();
        }
    };

    let prompt = format!(
        "Scenario: {} feels {} about {}.\n\n\
         (In the conversation, they discuss someone else who is experiencing {}.)\n\n\
         {}: [utterance]\n\n\
         {}: [response]\n\n\
         ...\n\n\
         Requirements:\n\
         1. {}'s {} exists ONLY in the scenario description\n\
         2. In the dialogue, {} hides their emotion completely\n\
         3. {} must explicitly discuss or mention someone else's {}\n\
         4. {}'s dialogue should be neutral about themselves\n\
         5. Keep it natural and grounded",
        name_a, real_emotion, topic, other_emotion,
        name_a, name_b, name_a, real_emotion, name_a,
        name_a, other_emotion, name_a
    );

    let system_prompt = format!(
        "Generate a dialogue where {} hides their {} emotion while discussing \
         someone else's {} emotion. The topic is {}. {}'s real emotion should \
         only be in the scenario description.",
        name_a, real_emotion, other_emotion, topic, name_a
    );

    EmotionPrompt {
        id: 0,
        mode: "unexpressed-others".to_string(),
        real_emotion: real_emotion.to_string(),
        displayed_emotion: None,
        story_emotion: None,
        other_emotion: Some(other_emotion),
        topic: topic.to_string(),
        conversation_topic: None,
        name_a: name_a.to_string(),
        name_b: name_b.to_string(),
        prompt,
        system_prompt,
    }
}

/// Generate a single prompt based on mode and parameters
fn generate_prompt(
    rng: &mut ThreadRng,
    mode: GenerationMode,
    emotion: &str,
    topics: &[String],
) -> EmotionPrompt {
    let topic = topics.choose(rng).unwrap().clone();
    let (name_a, name_b) = random_names(rng);

    match mode {
        GenerationMode::Natural => {
            generate_natural_prompt(rng, emotion, &topic, &name_a, &name_b)
        }
        GenerationMode::Deflection => {
            generate_deflection_prompt(rng, emotion, &topic, &name_a, &name_b)
        }
        GenerationMode::UnexpressedNeutral => {
            generate_unexpressed_neutral_prompt(rng, emotion, &topic, &name_a, &name_b)
        }
        GenerationMode::UnexpressedStory => {
            generate_unexpressed_story_prompt(rng, emotion, &topic, &name_a)
        }
        GenerationMode::UnexpressedOthers => {
            generate_unexpressed_others_prompt(rng, emotion, &topic, &name_a, &name_b)
        }
    }
}

/// Write prompts in JSONL format
fn write_jsonl(path: &std::path::Path, prompts: &[EmotionPrompt]) -> Result<()> {
    let mut file = std::fs::File::create(path).context("Failed to create JSONL file")?;
    for prompt in prompts {
        writeln!(file, "{}", serde_json::to_string(prompt)?)?;
    }
    Ok(())
}

/// Write prompts in JSON array format
fn write_json(path: &std::path::Path, prompts: &[EmotionPrompt]) -> Result<()> {
    let json = serde_json::to_string_pretty(prompts)?;
    std::fs::write(path, json).context("Failed to write JSON file")?;
    Ok(())
}

/// Write metadata summary
fn write_metadata(
    path: &std::path::Path,
    prompts: &[EmotionPrompt],
    args: &Args,
) -> Result<()> {
    let mut file = std::fs::File::create(path).context("Failed to create metadata file")?;

    writeln!(file, "# Emotion Activation Dataset Metadata")?;
    writeln!(file, "# Generated by emotion-activation-gen")?;
    writeln!(file, "")?;
    writeln!(file, "model: {}", args.model)?;
    writeln!(file, "layer: {}", args.layer)?;
    writeln!(file, "hidden_size: {}", args.hidden_size)?;
    writeln!(file, "mode: {:?}", args.mode)?;
    writeln!(file, "total_prompts: {}", prompts.len())?;
    writeln!(file, "emotions: [{}]", args.emotions.join(", "))?;
    writeln!(file, "")?;
    writeln!(file, "# Usage with EmotionExtractorNode:")?;
    writeln!(file, "# 1. Feed each prompt to the LLM to generate text")?;
    writeln!(file, "# 2. Capture activations at layer {} as RuntimeData::Tensor", args.layer)?;
    writeln!(file, "# 3. Tag each tensor with metadata: {{ \"emotion\": \"<real_emotion>\" }}")?;
    writeln!(file, "# 4. Send \"compute\" trigger to extract direction vectors")?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging
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

    // Load topics
    let topics = load_topics(&args)?;
    if topics.is_empty() {
        anyhow::bail!("No topics available");
    }

    // Create output directory
    std::fs::create_dir_all(&args.output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            args.output_dir.display()
        )
    })?;

    // Generate prompts
    let mut rng = thread_rng();
    let mut prompts: Vec<EmotionPrompt> = Vec::new();
    let mut global_id = 0;

    for emotion in &args.emotions {
        for _ in 0..args.samples {
            let mut prompt = generate_prompt(&mut rng, args.mode, emotion, &topics);
            prompt.id = global_id;
            prompts.push(prompt);
            global_id += 1;
        }
    }

    // Write output
    let output_file = match args.format.as_str() {
        "jsonl" => args.output_dir.join("prompts.jsonl"),
        "json" => args.output_dir.join("prompts.json"),
        "yaml" => args.output_dir.join("prompts.json"), // YAML not yet supported
        other => {
            anyhow::bail!("Unsupported format: {}. Use jsonl, json, or yaml", other)
        }
    };

    match args.format.as_str() {
        "jsonl" => write_jsonl(&output_file, &prompts)?,
        "json" => write_json(&output_file, &prompts)?,
        _ => write_json(&output_file, &prompts)?,
    }

    // Write metadata
    write_metadata(&args.output_dir.join("metadata.yaml"), &prompts, &args)?;

    // Summary
    if !args.quiet {
        eprintln!(
            "Generated {} prompts ({} emotions x {} samples)",
            prompts.len(),
            args.emotions.len(),
            args.samples
        );
        eprintln!("Output: {}", output_file.display());
        eprintln!("Metadata: {}", args.output_dir.join("metadata.yaml").display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_natural_prompt() {
        let mut rng = thread_rng();
        let prompt = generate_natural_prompt(&mut rng, "happy", "a vacation", "Alice", "Bob");
        assert_eq!(prompt.mode, "natural");
        assert_eq!(prompt.real_emotion, "happy");
        assert!(prompt.prompt.contains("Alice"));
        assert!(prompt.prompt.contains("happy"));
        assert!(prompt.prompt.contains("vacation"));
    }

    #[test]
    fn test_generate_deflection_prompt() {
        let mut rng = thread_rng();
        let prompt = generate_deflection_prompt(&mut rng, "sad", "work", "Alice", "Bob");
        assert_eq!(prompt.mode, "deflection");
        assert_eq!(prompt.real_emotion, "sad");
        assert!(prompt.displayed_emotion.is_some());
        assert_ne!(prompt.displayed_emotion.as_ref().unwrap(), &"sad");
    }

    #[test]
    fn test_generate_unexpressed_neutral_prompt() {
        let mut rng = thread_rng();
        let prompt = generate_unexpressed_neutral_prompt(&mut rng, "angry", "work", "Alice", "Bob");
        assert_eq!(prompt.mode, "unexpressed-neutral");
        assert_eq!(prompt.real_emotion, "angry");
        assert!(prompt.conversation_topic.is_some());
    }

    #[test]
    fn test_generate_unexpressed_story_prompt() {
        let mut rng = thread_rng();
        let prompt = generate_unexpressed_story_prompt(&mut rng, "sad", "work", "Alice");
        assert_eq!(prompt.mode, "unexpressed-story");
        assert_eq!(prompt.real_emotion, "sad");
        assert!(prompt.story_emotion.is_some());
        assert_ne!(prompt.story_emotion.as_ref().unwrap(), &"sad");
    }

    #[test]
    fn test_generate_unexpressed_others_prompt() {
        let mut rng = thread_rng();
        let prompt = generate_unexpressed_others_prompt(&mut rng, "angry", "work", "Alice", "Bob");
        assert_eq!(prompt.mode, "unexpressed-others");
        assert_eq!(prompt.real_emotion, "angry");
        assert!(prompt.other_emotion.is_some());
    }

    #[test]
    fn test_random_names_unique() {
        let mut rng = thread_rng();
        for _ in 0..100 {
            let (a, b) = random_names(&mut rng);
            assert_ne!(a, b, "Names should be unique");
        }
    }

    #[test]
    fn test_write_jsonl() {
        let dir = std::env::temp_dir().join("emotion_gen_test_jsonl");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.jsonl");

        let prompts = vec![
            EmotionPrompt {
                id: 0,
                mode: "natural".to_string(),
                real_emotion: "happy".to_string(),
                displayed_emotion: None,
                story_emotion: None,
                other_emotion: None,
                topic: "vacation".to_string(),
                conversation_topic: None,
                name_a: "Alice".to_string(),
                name_b: "Bob".to_string(),
                prompt: "test prompt".to_string(),
                system_prompt: "test system".to_string(),
            },
        ];

        write_jsonl(&path, &prompts).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"happy\""));
        assert!(content.contains("\"natural\""));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_write_json() {
        let dir = std::env::temp_dir().join("emotion_gen_test_json");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.json");

        let prompts = vec![
            EmotionPrompt {
                id: 0,
                mode: "deflection".to_string(),
                real_emotion: "sad".to_string(),
                displayed_emotion: Some("happy".to_string()),
                story_emotion: None,
                other_emotion: None,
                topic: "work".to_string(),
                conversation_topic: None,
                name_a: "Alice".to_string(),
                name_b: "Bob".to_string(),
                prompt: "test prompt".to_string(),
                system_prompt: "test system".to_string(),
            },
        ];

        write_json(&path, &prompts).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"sad\""));
        assert!(content.contains("\"deflection\""));

        std::fs::remove_dir_all(&dir).ok();
    }
}
