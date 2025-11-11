/**
 * Pipeline Builder Utilities
 *
 * Helper functions for creating pipeline manifests for speech-to-speech
 * and other streaming pipelines.
 */

import { processValue } from './resource-loader';

export interface PipelineNode {
  id: string;
  nodeType: string;
  params: string | Record<string, any>;
  isStreaming?: boolean;
  description?: string;
}

export interface PipelineConnection {
  from: string;
  to: string;
  description?: string;
}

export interface PipelineManifest {
  version: string;
  metadata: {
    name: string;
    description: string;
    createdAt: string;
    [key: string]: any;
  };
  nodes: PipelineNode[];
  connections: PipelineConnection[];
  config?: Record<string, any>;
}

/**
 * Create a simple S2S pipeline (LFM2 text-only + Kokoro TTS)
 */
export function createSimpleS2SPipeline(options: {
  sessionId: string;
  systemPrompt?: string;
  audioTemperature?: number;
  audioTopK?: number;
  maxNewTokens?: number;
}): PipelineManifest {
  const {
    sessionId,
    systemPrompt = 'Respond with interleaved text and audio.',
    audioTemperature = 1.0,
    audioTopK = 4,
    maxNewTokens = 4096,
  } = options;

  return {
    version: 'v1',
    metadata: {
      name: 'simple-s2s',
      description: `Simple S2S Session: ${sessionId}`,
      createdAt: new Date().toISOString(),
    },
    nodes: [
      {
        id: 'lfm2_audio',
        nodeType: 'LFM2AudioNode',
        params: JSON.stringify({
          hf_repo: 'liquidai/LFM2-Audio-1.5B',
          device: 'cuda',
          system_prompt: systemPrompt,
          text_only: true,
          text_temperature: 0.7,
          text_top_k: 50,
          max_new_tokens: maxNewTokens,
        }),
        isStreaming: true,
      },
      {
        id: 'text_collector',
        nodeType: 'TextCollectorNode',
        params: JSON.stringify({
          splitPattern: '[.!?;\n]+',
          minSentenceLength: 3,
          yieldPartialOnEnd: true,
        }),
        isStreaming: true,
        description: 'Collect streaming text tokens into complete sentences',
      },
      {
        id: 'kokoro_tts',
        nodeType: 'KokoroTTSNode',
        params: JSON.stringify({
          model_id: 'kokoro-v0_19',
          voice: 'af_bella',
          speed: 1.0,
          device: 'cuda',
        }),
        isStreaming: true,
      },
    ],
    connections: [
      { from: 'lfm2_audio', to: 'text_collector', description: 'Stream text tokens to collector' },
      { from: 'text_collector', to: 'kokoro_tts', description: 'Complete sentences to TTS' },
    ],
    config: {
      execution_mode: 'streaming',
      enable_caching: true,
      cache_ttl_seconds: 1800,
    },
  };
}

/**
 * Create a VibeVoice TTS-only pipeline (text input → streamed audio)
 */
export async function createVibeVoiceTTSPipeline(options: {
  sessionId: string;
  modelPath?: string;
  device?: 'cuda' | 'mps' | 'cpu';
  cfgScale?: number;
  inferenceSteps?: number;
  sampleRate?: number;
  useVoiceCloning?: boolean;
  voiceSamples?: string[]; // absolute or container-visible paths
  systemPrompt?: string;
  maxNewTokens?: number;
  vadThreshold?: number;
  minSpeechDurationMs?: number;
  minSilenceDurationMs?: number;
  minUtteranceDurationMs?: number;
  maxUtteranceDurationMs?: number;
  chunkSize?: number;
}): Promise<PipelineManifest> {
  console.log('🔴 [PIPELINE-BUILDER] createVibeVoiceTTSPipeline CALLED!');
  return createVADS2SPipeline(options);
  const {
    sessionId,
    systemPrompt = 'Respond with interleaved text and audio.',
    maxNewTokens = 4096,
    vadThreshold = 0.5,
    minSpeechDurationMs = 250,
    minSilenceDurationMs = 500,
    minUtteranceDurationMs = 500,
    maxUtteranceDurationMs = 30000,
    chunkSize = 512,
  } = options;

    const {
    modelPath = 'vibevoice/VibeVoice-1.5B',
    device = 'cuda',
    cfgScale = 1.3,
    inferenceSteps = 10,
    sampleRate = 24000,
    useVoiceCloning = true,
    voiceSamples = ['require:C:/Users/mail/dev/personal/remotemedia-sdk/examples/transcribe_demo.wav'],
  } = options;

  // Process voice samples - resolve require() calls to embed audio data
  const processedVoiceSamples = await processValue(voiceSamples);

  return {
    version: 'v1',
    metadata: {
      name: 'vad-s2s-streaming',
      description: `VAD S2S Session: ${sessionId}`,
      createdAt: new Date().toISOString(),
    },
    nodes: [
      {
        id: 'input_chunker',
        nodeType: 'AudioChunkerNode',
        params: JSON.stringify({
          chunkSize: 1024, // Split input to match resampler chunk size
        }),
        isStreaming: true,
        description: 'Split browser audio into 1024-sample chunks @ 48kHz for resampler',
      },
      {
        id: 'audio_resampler',
        nodeType: 'FastResampleNode',
        params: JSON.stringify({
          sourceRate: 48000, // Browser typically provides 48kHz
          targetRate: 16000, // VAD requires 16kHz
          quality: 'Medium', // Medium quality, 1024-sample input chunks @ 48kHz
          channels: 1,
        }),
        isStreaming: true,
        description: 'Resample 48kHz → 16kHz (1024 samples @ 48kHz → ~341 samples @ 16kHz)',
      },
      {
        id: 'vad_chunker',
        nodeType: 'AudioChunkerNode',
        params: JSON.stringify({
          chunkSize: 512, // 512 samples @ 16kHz for VAD
        }),
        isStreaming: true,
        description: 'Split resampled audio into 512-sample chunks @ 16kHz for VAD',
      },
      {
        id: 'silero_vad',
        nodeType: 'SileroVADNode',
        params: JSON.stringify({
          threshold: vadThreshold,
          samplingRate: 16000,
          minSpeechDurationMs: minSpeechDurationMs,
          minSilenceDurationMs: minSilenceDurationMs,
          speechPadMs: 30,
        }),
        isStreaming: true,
        description: 'Silero VAD for speech detection',
      },
      {
        id: 'audio_buffer',
        nodeType: 'AudioBufferAccumulatorNode',
        params: JSON.stringify({
          minUtteranceDurationMs: minUtteranceDurationMs,
          maxUtteranceDurationMs: maxUtteranceDurationMs,
        }),
        isStreaming: true,
        description: 'Accumulate audio into complete utterances',
      },
      {
        id: 'vad_to_buffer_resampler',
        nodeType: 'FastResampleNode',
        params: JSON.stringify({
          sourceRate: 16000, // Browser typically provides 48kHz
          targetRate: 24000, // LFM2 requires 24kHz
          quality: 'Medium', // Medium quality, 1024-sample input chunks @ 48kHz
          channels: 1,
        }),
        isStreaming: true,
        description: 'Resample 16kHz → 24kHz (1024 samples @ 48kHz → ~341 samples @ 16kHz)',
      },
      {
        id: 'lfm2_audio',
        nodeType: 'LFM2AudioNode',
        params: JSON.stringify({
          hf_repo: 'liquidai/LFM2-Audio-1.5B',
          device: 'cuda',
          system_prompt: systemPrompt,
          text_only: false,
          text_temperature: 0.7,
          text_top_k: 50,
          maxNewTokens,
          max_new_tokens: maxNewTokens,
        }),
        isStreaming: true,
        description: 'ASR and conversational AI',
      },
      {
        id: 'text_collector',
        nodeType: 'TextCollectorNode',
        params: JSON.stringify({
          splitPattern: '[.!?;\n]+',
          minSentenceLength: 3,
          yieldPartialOnEnd: true,
        }),
        isStreaming: true,
        description: 'Collect generated text outputs into complete sentences',
      },
      {
        id: 'vibevoice_tts',
        nodeType: 'VibeVoiceTTSNode',
        params: JSON.stringify({
          model_path: modelPath,
          device,
          cfg_scale: cfgScale,
          inference_steps: inferenceSteps,
          sample_rate: sampleRate,
          use_voice_cloning: useVoiceCloning,
          voice_samples: processedVoiceSamples,
        }),
        isStreaming: true,
        description: 'VibeVoice-based text-to-speech synthesis',
      },
    ],
    connections: [
      { from: 'input_chunker', to: 'audio_resampler', description: '1024 samples @ 48kHz → resampler' },
      { from: 'audio_resampler', to: 'vad_chunker', description: '~341 samples @ 16kHz → VAD chunker' },
      { from: 'vad_chunker', to: 'silero_vad', description: '512 samples @ 16kHz → VAD' },
      { from: 'silero_vad', to: 'audio_buffer', description: 'Speech chunks → buffer' },
      { from: 'audio_buffer', to: 'vad_to_buffer_resampler', description: 'Complete utterances to LFM2' },
      { from: 'vad_to_buffer_resampler', to: 'lfm2_audio', description: '241 samples @ 24kHz → buffer' },
      { from: 'lfm2_audio', to: 'text_collector', description: 'Text tokens → collector' },
      { from: 'text_collector', to: 'vibevoice_tts', description: 'Complete sentences → TTS' },
    ],
    config: {
      execution_mode: 'streaming',
      enable_caching: true,
      cache_ttl_seconds: 1800,
      max_concurrent_sessions: 100,
    },
  };
}

/**
 * Create a VAD-based S2S pipeline (AudioChunker + Silero VAD + AudioBuffer + LFM2 + Kokoro)
 */
export function createVADS2SPipeline(options: {
  sessionId: string;
  systemPrompt?: string;
  maxNewTokens?: number;
  vadThreshold?: number;
  minSpeechDurationMs?: number;
  minSilenceDurationMs?: number;
  minUtteranceDurationMs?: number;
  maxUtteranceDurationMs?: number;
  chunkSize?: number;
}): PipelineManifest {
  console.log('🟢 [PIPELINE-BUILDER] createVADS2SPipeline CALLED (should use KokoroTTS)!');
  const {
    sessionId,
    systemPrompt = 'Respond with interleaved text and audio.',
    maxNewTokens = 4096,
    vadThreshold = 0.5,
    minSpeechDurationMs = 250,
    minSilenceDurationMs = 500,
    minUtteranceDurationMs = 500,
    maxUtteranceDurationMs = 30000,
    chunkSize = 512,
  } = options;

  return {
    version: 'v1',
    metadata: {
      name: 'vad-s2s-streaming',
      description: `VAD S2S Session: ${sessionId}`,
      createdAt: new Date().toISOString(),
    },
    nodes: [
      {
        id: 'input_chunker',
        nodeType: 'AudioChunkerNode',
        params: JSON.stringify({
          chunkSize: 1024, // Split input to match resampler chunk size
        }),
        isStreaming: true,
        description: 'Split browser audio into 1024-sample chunks @ 48kHz for resampler',
      },
      {
        id: 'audio_resampler',
        nodeType: 'FastResampleNode',
        params: JSON.stringify({
          sourceRate: 48000, // Browser typically provides 48kHz
          targetRate: 16000, // VAD requires 16kHz
          quality: 'Medium', // Medium quality, 1024-sample input chunks @ 48kHz
          channels: 1,
        }),
        isStreaming: true,
        description: 'Resample 48kHz → 16kHz (1024 samples @ 48kHz → ~341 samples @ 16kHz)',
      },
      {
        id: 'vad_chunker',
        nodeType: 'AudioChunkerNode',
        params: JSON.stringify({
          chunkSize: 512, // 512 samples @ 16kHz for VAD
        }),
        isStreaming: true,
        description: 'Split resampled audio into 512-sample chunks @ 16kHz for VAD',
      },
      {
        id: 'silero_vad',
        nodeType: 'SileroVADNode',
        params: JSON.stringify({
          threshold: vadThreshold,
          samplingRate: 16000,
          minSpeechDurationMs: minSpeechDurationMs,
          minSilenceDurationMs: minSilenceDurationMs,
          speechPadMs: 30,
        }),
        isStreaming: true,
        description: 'Silero VAD for speech detection',
      },
      {
        id: 'audio_buffer',
        nodeType: 'AudioBufferAccumulatorNode',
        params: JSON.stringify({
          minUtteranceDurationMs: minUtteranceDurationMs,
          maxUtteranceDurationMs: maxUtteranceDurationMs,
        }),
        isStreaming: true,
        description: 'Accumulate audio into complete utterances',
      },
      {
        id: 'vad_to_buffer_resampler',
        nodeType: 'FastResampleNode',
        params: JSON.stringify({
          sourceRate: 16000, // Browser typically provides 48kHz
          targetRate: 24000, // LFM2 requires 24kHz
          quality: 'Medium', // Medium quality, 1024-sample input chunks @ 48kHz
          channels: 1,
        }),
        isStreaming: true,
        description: 'Resample 16kHz → 24kHz (1024 samples @ 48kHz → ~341 samples @ 16kHz)',
      },
      {
        id: 'lfm2_audio',
        nodeType: 'LFM2AudioNode',
        params: JSON.stringify({
          hf_repo: 'liquidai/LFM2-Audio-1.5B',
          device: 'cuda',
          system_prompt: systemPrompt,
          text_only: false,
          text_temperature: 0.7,
          text_top_k: 50,
          maxNewTokens,
          max_new_tokens: maxNewTokens,
        }),
        isStreaming: true,
        description: 'ASR and conversational AI',
      },
      // {
      //   id: 'text_collector',
      //   nodeType: 'TextCollectorNode',
      //   params: JSON.stringify({
      //     splitPattern: '[.!?;\n]+',
      //     minSentenceLength: 3,
      //     yieldPartialOnEnd: true,
      //   }),
      //   isStreaming: true,
      //   description: 'Collect generated text outputs into complete sentences',
      // },
      // {
      //   id: 'kokoro_tts',
      //   nodeType: 'KokoroTTSNode',
      //   params: JSON.stringify({
      //     model_id: 'kokoro-v0_19',
      //     voice: 'af_bella',
      //     speed: 1.0,
      //   }),
      //   isStreaming: true,
      //   description: 'Text-to-speech synthesis',
      // }
    ],
    connections: [
      { from: 'input_chunker', to: 'audio_resampler', description: '1024 samples @ 48kHz → resampler' },
      { from: 'audio_resampler', to: 'vad_chunker', description: '~341 samples @ 16kHz → VAD chunker' },
      { from: 'vad_chunker', to: 'silero_vad', description: '512 samples @ 16kHz → VAD' },
      { from: 'silero_vad', to: 'audio_buffer', description: 'Speech chunks → buffer' },
      { from: 'audio_buffer', to: 'vad_to_buffer_resampler', description: 'Complete utterances to LFM2' },
      { from: 'vad_to_buffer_resampler', to: 'lfm2_audio', description: '241 samples @ 24kHz → buffer' },
      // { from: 'lfm2_audio', to: 'text_collector', description: 'Text tokens → collector' },
      // { from: 'text_collector', to: 'kokoro_tts', description: 'Complete sentences → TTS' },
    ],
    config: {
      execution_mode: 'streaming',
      enable_caching: true,
      cache_ttl_seconds: 1800,
      max_concurrent_sessions: 100,
    },
  };
}

/**
 * Create a TTS pipeline
 */
export function createTTSPipeline(options: {
  text: string;
  language?: string;
  voice?: string;
  speed?: number;
}): PipelineManifest {
  const { text, language = 'en-us', voice = 'af_bella', speed = 1.0 } = options;

  return {
    version: 'v1',
    metadata: {
      name: 'tts-streaming',
      description: `TTS: ${text.substring(0, 50)}`,
      createdAt: new Date().toISOString(),
    },
    nodes: [
      {
        id: 'tts',
        nodeType: 'KokoroTTSNode',
        params: JSON.stringify({
          text,
          language,
          voice,
          speed,
        }),
        isStreaming: true,
      },
    ],
    connections: [],
  };
}

/**
 * Create a VAD Debug pipeline (VAD → Audio Output)
 *
 * This pipeline processes audio through VAD and returns the detected speech segments
 * without sending to LFM2Audio. Useful for debugging VAD and audio quality.
 */
export function createVADDebugPipeline(options: {
  sessionId: string;
  vadThreshold?: number;
  minSpeechDurationMs?: number;
  minSilenceDurationMs?: number;
  minUtteranceDurationMs?: number;
  maxUtteranceDurationMs?: number;
}): PipelineManifest {
  const {
    sessionId,
    vadThreshold = 0.5,
    minSpeechDurationMs = 750,
    minSilenceDurationMs = 500,
    minUtteranceDurationMs = 500,
    maxUtteranceDurationMs = 30000,
  } = options;

  return {
    version: 'v1',
    metadata: {
      name: 'vad-debug',
      description: `VAD Debug Pipeline: ${sessionId}`,
      createdAt: new Date().toISOString(),
    },
    nodes: [
      {
        id: 'input_chunker',
        nodeType: 'AudioChunkerNode',
        params: JSON.stringify({
          chunkSize: 1024, // Split input to match resampler chunk size
        }),
        isStreaming: true,
        description: 'Split browser audio into 1024-sample chunks @ 48kHz for resampler',
      },
      {
        id: 'audio_resampler',
        nodeType: 'FastResampleNode',
        params: JSON.stringify({
          sourceRate: 48000,
          targetRate: 16000,
          quality: 'Medium',
          channels: 1,
        }),
        isStreaming: true,
        description: 'Resample 48kHz → 16kHz',
      },
      {
        id: 'vad_chunker',
        nodeType: 'AudioChunkerNode',
        params: JSON.stringify({
          chunkSize: 512, // 512 samples @ 16kHz for VAD
        }),
        isStreaming: true,
        description: 'Split resampled audio into 512-sample chunks @ 16kHz for VAD',
      },
      {
        id: 'silero_vad',
        nodeType: 'SileroVADNode',
        params: JSON.stringify({
          threshold: vadThreshold,
          samplingRate: 16000,
          minSpeechDurationMs: minSpeechDurationMs,
          minSilenceDurationMs: minSilenceDurationMs,
          speechPadMs: 30,
        }),
        isStreaming: true,
        description: 'Silero VAD for speech detection',
      },
      {
        id: 'audio_buffer',
        nodeType: 'AudioBufferAccumulatorNode',
        params: JSON.stringify({
          minUtteranceDurationMs: minUtteranceDurationMs,
          maxUtteranceDurationMs: maxUtteranceDurationMs,
        }),
        isStreaming: true,
        description: 'Accumulate audio into complete utterances',
      },
      {
        id: 'vad_to_buffer_resampler',
        nodeType: 'FastResampleNode',
        params: JSON.stringify({
          sourceRate: 16000, // Browser typically provides 48kHz
          targetRate: 24000, // LFM2 requires 24kHz
          quality: 'Medium', // Medium quality, 1024-sample input chunks @ 48kHz
          channels: 1,
        }),
        isStreaming: true,
        description: 'Resample 16kHz → 24kHz (1024 samples @ 48kHz → ~341 samples @ 16kHz)',
      },
    ],
    connections: [
      { from: 'input_chunker', to: 'audio_resampler', description: '1024 samples @ 48kHz → resampler' },
      { from: 'audio_resampler', to: 'vad_chunker', description: '~341 samples @ 16kHz → VAD chunker' },
      { from: 'vad_chunker', to: 'silero_vad', description: '512 samples @ 16kHz → VAD' },
      { from: 'silero_vad', to: 'audio_buffer', description: 'Speech chunks → buffer' },
      { from: 'audio_buffer', to: 'vad_to_buffer_resampler', description: 'Complete utterances to LFM2' },
    ],
    config: {
      execution_mode: 'streaming',
      enable_caching: false, // Disable caching for debugging
      max_concurrent_sessions: 10,
    },
  };
}

/**
 * Create an optimized low-latency pipeline with SpeculativeVADGate
 *
 * This pipeline uses speculative forwarding for smooth, real-time audio with
 * 3.7x lower latency compared to traditional VAD.
 *
 * Measured performance:
 * - Traditional: 22.48ms per chunk (choppy, stuttery)
 * - Speculative: 6.01ms per chunk (smooth, natural)
 * - Improvement: 3.74x faster, eliminates choppiness
 */
export function createSpeculativeVADS2SPipeline(options: {
  sessionId: string;
  systemPrompt?: string;
  maxNewTokens?: number;
  vadThreshold?: number;
  minSpeechDurationMs?: number;
  minSilenceDurationMs?: number;
  minUtteranceDurationMs?: number;
  maxUtteranceDurationMs?: number;
  lookbackMs?: number;
  lookaheadMs?: number;
}): PipelineManifest {
  console.log('⚡ [PIPELINE-BUILDER] createSpeculativeVADS2SPipeline - LOW LATENCY MODE!');
  const {
    sessionId,
    systemPrompt = 'Respond with interleaved text and audio.',
    maxNewTokens = 4096,
    vadThreshold = 0.5,
    minSpeechDurationMs = 200,
    minSilenceDurationMs = 300,
    minUtteranceDurationMs = 500,
    maxUtteranceDurationMs = 30000,
    lookbackMs = 100,
    lookaheadMs = 25,
  } = options;

  return {
    version: 'v1',
    metadata: {
      name: 'speculative-vad-s2s-streaming',
      description: `Speculative VAD S2S (Low Latency): ${sessionId}`,
      createdAt: new Date().toISOString(),
      optimization: 'speculative_vad_forwarding',
      expectedLatency: '<250ms P99',
    },
    nodes: [
      {
        id: 'input_chunker',
        nodeType: 'AudioChunkerNode',
        params: JSON.stringify({
          chunkSize: 960,
        }),
        isStreaming: true,
        description: 'Split browser audio @ 48kHz',
      },
      {
        id: 'audio_resampler',
        nodeType: 'FastResampleNode',
        params: JSON.stringify({
          sourceRate: 48000,
          targetRate: 16000,
          quality: 'Low',
          channels: 1,
        }),
        isStreaming: true,
        description: 'Resample 48kHz → 16kHz',
      },
      {
        id: 'vad_chunker',
        nodeType: 'AudioChunkerNode',
        params: JSON.stringify({
          chunkSize: 320,
        }),
        isStreaming: true,
        description: 'Chunk for VAD @ 16kHz',
      },
      {
        id: 'speculative_vad',
        nodeType: 'SpeculativeVADGate',
        params: JSON.stringify({
          lookbackMs: lookbackMs,
          lookaheadMs: lookaheadMs,
          vadThreshold: vadThreshold,
          sampleRate: 16000,
          minSpeechMs: minSpeechDurationMs,
          minSilenceMs: minSilenceDurationMs,
          padMs: 12,
        }),
        isStreaming: true,
        description: '⚡ Speculative VAD - immediate forwarding with retroactive cancellation',
      },
      {
        id: 'confirmation_vad',
        nodeType: 'SileroVADNode',
        params: JSON.stringify({
          threshold: vadThreshold,
          samplingRate: 16000,
          minSpeechDurationMs: minSpeechDurationMs,
          minSilenceDurationMs: minSilenceDurationMs,
          speechPadMs: 30,
        }),
        isStreaming: true,
        description: 'VAD confirmation (runs in parallel)',
      },
      {
        id: 'audio_buffer',
        nodeType: 'AudioBufferAccumulatorNode',
        params: JSON.stringify({
          minUtteranceDurationMs: minUtteranceDurationMs,
          maxUtteranceDurationMs: maxUtteranceDurationMs,
        }),
        isStreaming: true,
        description: 'Accumulate complete utterances (16kHz)',
      },
      {
        id: 'buffer_resampler',
        nodeType: 'FastResampleNode',
        params: JSON.stringify({
          sourceRate: 16000,
          targetRate: 24000,
          quality: 'Medium',
          channels: 1,
        }),
        isStreaming: true,
        description: 'Resample 16kHz → 24kHz for LFM2 (after accumulation)',
      },
      {
        id: 'lfm2_audio',
        nodeType: 'LFM2AudioNode',
        params: JSON.stringify({
          hf_repo: 'liquidai/LFM2-Audio-1.5B',
          device: 'cuda',
          system_prompt: systemPrompt,
          text_only: false,
          text_temperature: 0.7,
          text_top_k: 50,
          max_new_tokens: maxNewTokens,
        }),
        isStreaming: true,
        description: 'Audio-native conversational AI',
      },
    ],
    connections: [
      { from: 'input_chunker', to: 'audio_resampler', description: '48kHz → resampler' },
      { from: 'audio_resampler', to: 'vad_chunker', description: '16kHz → chunker' },
      { from: 'vad_chunker', to: 'speculative_vad', description: '320-sample chunks' },

      // Speculative forwarding path (immediate, <50μs)
      { from: 'speculative_vad', to: 'audio_buffer', description: '⚡ Immediate forward (16kHz)' },

      // Confirmation path (parallel, doesn't block)
      { from: 'vad_chunker', to: 'confirmation_vad', description: 'VAD confirmation (parallel)' },
      { from: 'confirmation_vad', to: 'speculative_vad', description: 'Send control messages' },

      // Continue pipeline
      { from: 'audio_buffer', to: 'buffer_resampler', description: 'Resample 16kHz → 24kHz' },
      { from: 'buffer_resampler', to: 'lfm2_audio', description: '24kHz to LFM2' },
    ],
    config: {
      execution_mode: 'streaming',
      enable_caching: true,
      cache_ttl_seconds: 1800,
      max_concurrent_sessions: 100,
      enable_metrics: true,
      metrics_port: 9090,
    },
  };
}

/**
 * Validate pipeline manifest
 */
export function validatePipelineManifest(manifest: PipelineManifest): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  // Check required fields
  if (!manifest.version) {
    errors.push('Missing version field');
  }

  if (!manifest.metadata?.name) {
    errors.push('Missing metadata.name field');
  }

  if (!manifest.nodes || manifest.nodes.length === 0) {
    errors.push('Pipeline must have at least one node');
  }

  // Validate nodes
  const nodeIds = new Set<string>();
  for (const node of manifest.nodes || []) {
    if (!node.id) {
      errors.push('Node missing id field');
    } else if (nodeIds.has(node.id)) {
      errors.push(`Duplicate node id: ${node.id}`);
    } else {
      nodeIds.add(node.id);
    }

    if (!node.nodeType) {
      errors.push(`Node ${node.id} missing nodeType field`);
    }
  }

  // Validate connections
  for (const conn of manifest.connections || []) {
    if (!conn.from) {
      errors.push('Connection missing from field');
    } else if (!nodeIds.has(conn.from)) {
      errors.push(`Connection references non-existent node: ${conn.from}`);
    }

    if (!conn.to) {
      errors.push('Connection missing to field');
    } else if (!nodeIds.has(conn.to)) {
      errors.push(`Connection references non-existent node: ${conn.to}`);
    }
  }

  return {
    valid: errors.length === 0,
    errors,
  };
}

/**
 * Load pipeline manifest from JSON file
 */
export async function loadPipelineManifest(filePath: string): Promise<PipelineManifest> {
  const fs = await import('fs/promises');
  const content = await fs.readFile(filePath, 'utf-8');
  const manifest = JSON.parse(content) as PipelineManifest;

  const validation = validatePipelineManifest(manifest);
  if (!validation.valid) {
    throw new Error(`Invalid pipeline manifest: ${validation.errors.join(', ')}`);
  }

  return manifest;
}
