# emotion-activation-gen

Generate emotion-labelled prompts for activation extraction datasets.

## Overview

This tool generates dialogue scenarios with explicit emotion labels, suitable for feeding into an LLM to capture residual-stream activations for emotion vector extraction. It supports multiple generation modes:

- **Natural**: Emotion matches dialogue naturally
- **Deflection**: Real emotion hidden, displayed emotion shown
- **UnexpressedNeutral**: Real emotion exists only in scenario, conversation is neutral
- **UnexpressedStory**: Real emotion differs from story's emotion
- **UnexpressedOthers**: Real emotion hidden while discussing others' emotions

## Usage

```bash
# Generate 50 happy/neutral prompts for activation capture
cargo run -p emotion-activation-gen -- --emotions happy,neutral --samples 25 --output-dir ./prompts

# Generate deflection prompts (real vs displayed emotion)
cargo run -p emotion-activation-gen -- --mode deflection --emotions happy,sad,angry --samples 10

# Generate from a specific topic list
cargo run -p emotion-activation-gen -- --topic-file topics.txt --emotions happy,sad --samples 5

# Generate unexpressed emotion prompts
cargo run -p emotion-activation-gen -- --mode unexpressed-neutral --emotions angry,sad --samples 20
```

## Output Format

The tool generates:
- `prompts.jsonl`: One JSON object per line with prompt metadata
- `metadata.yaml`: Dataset metadata and usage instructions

Each prompt contains:
```json
{
  "id": 0,
  "mode": "natural",
  "real_emotion": "happy",
  "topic": "weekend hiking trip",
  "name_a": "Frank",
  "name_b": "Tina",
  "prompt": "Scenario: Frank feels happy about weekend hiking trip...",
  "system_prompt": "Generate a natural dialogue between Frank and Tina..."
}
```

## Workflow

1. **Generate prompts**: Use `emotion-activation-gen` to create labelled prompts
2. **Generate text**: Feed prompts to an LLM to generate dialogue text
3. **Capture activations**: Extract residual-stream activations at target layer
4. **Tag tensors**: Tag each activation tensor with emotion metadata
5. **Extract vectors**: Feed tensors to `EmotionExtractorNode`
6. **Compute vectors**: Trigger computation with "compute" message
7. **Apply steering**: Use extracted vectors in `EmotionSteeringNode`

## Integration with Emotion Vector Pipeline

```bash
# Step 1: Generate prompts
emotion-activation-gen --emotions happy,sad,neutral --samples 20 --output-dir ./prompts

# Step 2: (External) Feed prompts to LLM, capture activations as RuntimeData::Tensor
# Each tensor carries metadata: { "emotion": "<real_emotion>" }

# Step 3: Feed labelled activations to EmotionExtractorNode
# See examples/cli/pipelines/emotion-vector.yaml for pipeline configuration

# Step 4: Trigger vector computation
# Send "compute" message to extractor node

# Step 5: Vectors flow to EmotionSteeringNode automatically
# Apply steering during inference with coefficient updates
```

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `--emotions` | happy,sad,angry,fearful,neutral | Emotions to generate |
| `--samples` | 20 | Samples per emotion |
| `--mode` | natural | Generation mode |
| `--model` | meta-llama/Llama-3.1-8B-Instruct | Target LLM model |
| `--layer` | 21 | Target layer for activation capture |
| `--hidden-size` | 4096 | Model hidden dimension |
| `--format` | jsonl | Output format (jsonl, json) |

## References

- [Transformer Circuits: Emotions](https://transformer-circuits.pub/2026/emotions/index.html)
- [llm_feeling_weather](https://github.com/mezoistvan/llm_feeling_weather)
- [Emotion Vector Pipeline](../pipelines/emotion-vector.yaml)
