/**
 * Audio Type Definitions
 * Based on data-model.md specification
 */

/**
 * Playback state values
 */
export type PlaybackState = 'idle' | 'playing' | 'paused' | 'stopped' | 'buffering';

/**
 * Buffer health status
 */
export type BufferStatus = 'healthy' | 'low' | 'critical' | 'empty' | 'complete';

/**
 * Audio format specification
 */
export interface AudioFormat {
  /** Sample rate in Hz (e.g., 24000 for Kokoro TTS) */
  sampleRate: number;
  /** Number of channels (1 = mono, 2 = stereo) */
  channels: number;
  /** Bits per sample (e.g., 16, 24, 32) */
  bitDepth: number;
  /** Audio encoding format */
  encoding: 'pcm_f32le' | 'pcm_s16le' | 'pcm_s24le' | 'pcm_s32le';
}

/**
 * Audio chunk metadata
 */
export interface AudioChunk {
  /** Chunk sequence number (0-indexed) */
  sequenceNumber: number;
  /** Raw audio data (Float32Array for PCM) */
  data: Float32Array;
  /** Duration of this chunk in seconds */
  duration: number;
  /** Timestamp when chunk was received */
  receivedAt: Date;
  /** Optional chunk metadata */
  metadata?: {
    /** Total chunks expected (if known) */
    totalChunks?: number;
    /** Is this the final chunk? */
    isFinal?: boolean;
  };
}

/**
 * Buffer health metrics
 */
export interface BufferHealth {
  /** Current buffer status */
  status: BufferStatus;
  /** Seconds of audio buffered ahead of playhead */
  bufferedAhead: number;
  /** Target buffer duration in seconds (2.0s recommended) */
  targetBuffer: number;
  /** Total audio duration buffered (all chunks) */
  totalBuffered: number;
  /** Current playhead position in seconds */
  playheadPosition: number;
}

/**
 * Playback control state
 */
export interface PlaybackControl {
  /** Current playback state */
  state: PlaybackState;
  /** Volume level (0.0 to 1.0) */
  volume: number;
  /** Is audio muted? */
  muted: boolean;
  /** Playback position in seconds */
  currentTime: number;
  /** Total audio duration in seconds (if known) */
  duration: number | null;
}

/**
 * Audio stream state
 */
export interface AudioStreamState {
  /** Request ID this stream belongs to */
  requestId: string;
  /** Audio format specification */
  format: AudioFormat;
  /** Buffered audio chunks */
  chunks: AudioChunk[];
  /** Current playback control state */
  playbackControl: PlaybackControl;
  /** Buffer health metrics */
  bufferHealth: BufferHealth;
  /** Total bytes received */
  bytesReceived: number;
  /** Stream start timestamp */
  startedAt: Date | null;
  /** Stream end timestamp */
  completedAt: Date | null;
}

/**
 * Audio player event types
 */
export const AudioPlayerEvent = {
  PlaybackStarted: 'playback-started',
  PlaybackPaused: 'playback-paused',
  PlaybackStopped: 'playback-stopped',
  PlaybackResumed: 'playback-resumed',
  PlaybackEnded: 'playback-ended',
  BufferLow: 'buffer-low',
  BufferCritical: 'buffer-critical',
  BufferHealthy: 'buffer-healthy',
  ChunkReceived: 'chunk-received',
  StreamCompleted: 'stream-completed',
  Error: 'error',
  VolumeChanged: 'volume-changed',
  Seeked: 'seeked',
} as const;

export type AudioPlayerEventType = typeof AudioPlayerEvent[keyof typeof AudioPlayerEvent];

/**
 * Audio player event data
 */
export type AudioPlayerEventData = {
  [AudioPlayerEvent.PlaybackStarted]: { requestId: string };
  [AudioPlayerEvent.PlaybackPaused]: { requestId: string; currentTime: number };
  [AudioPlayerEvent.PlaybackStopped]: { requestId: string };
  [AudioPlayerEvent.PlaybackResumed]: { requestId: string; currentTime: number };
  [AudioPlayerEvent.PlaybackEnded]: { requestId: string; duration: number };
  [AudioPlayerEvent.BufferLow]: { requestId: string; bufferedAhead: number };
  [AudioPlayerEvent.BufferCritical]: { requestId: string; bufferedAhead: number };
  [AudioPlayerEvent.BufferHealthy]: { requestId: string; bufferedAhead: number };
  [AudioPlayerEvent.ChunkReceived]: { requestId: string; sequenceNumber: number; duration: number };
  [AudioPlayerEvent.StreamCompleted]: { requestId: string; totalDuration: number };
  [AudioPlayerEvent.Error]: { requestId: string; error: Error };
  [AudioPlayerEvent.VolumeChanged]: { volume: number; muted: boolean };
  [AudioPlayerEvent.Seeked]: { requestId: string; currentTime: number };
};

/**
 * Default audio format for Kokoro TTS
 */
export const DEFAULT_AUDIO_FORMAT: AudioFormat = {
  sampleRate: 24000,
  channels: 1,
  bitDepth: 32,
  encoding: 'pcm_f32le',
};

/**
 * Audio buffer configuration
 */
export const AUDIO_BUFFER_CONFIG = {
  /** Target buffer size in seconds */
  TARGET_BUFFER_SIZE: 2.0,
  /** Low buffer threshold in seconds */
  LOW_BUFFER_THRESHOLD: 1.0,
  /** Critical buffer threshold in seconds */
  CRITICAL_BUFFER_THRESHOLD: 0.5,
  /** Minimum buffer before starting playback */
  MIN_BUFFER_BEFORE_PLAYBACK: 0.5,
} as const;

/**
 * Audio constraints
 */
export const AUDIO_CONSTRAINTS = {
  /** Minimum volume */
  MIN_VOLUME: 0.0,
  /** Maximum volume */
  MAX_VOLUME: 1.0,
  /** Default volume */
  DEFAULT_VOLUME: 0.8,
} as const;

/**
 * Type guard to check if playback is active
 */
export function isPlaybackActive(state: PlaybackState): boolean {
  return ['playing', 'buffering'].includes(state);
}

/**
 * Type guard to check if buffer needs attention
 */
export function needsBuffering(status: BufferStatus): boolean {
  return ['low', 'critical', 'empty'].includes(status);
}

/**
 * Calculate buffer health status from metrics
 */
export function calculateBufferStatus(bufferedAhead: number, isComplete: boolean): BufferStatus {
  if (isComplete) {
    return 'complete';
  }
  if (bufferedAhead === 0) {
    return 'empty';
  }
  if (bufferedAhead < AUDIO_BUFFER_CONFIG.CRITICAL_BUFFER_THRESHOLD) {
    return 'critical';
  }
  if (bufferedAhead < AUDIO_BUFFER_CONFIG.LOW_BUFFER_THRESHOLD) {
    return 'low';
  }
  return 'healthy';
}

/**
 * Validate volume level
 */
export function validateVolume(volume: number): number {
  return Math.max(
    AUDIO_CONSTRAINTS.MIN_VOLUME,
    Math.min(AUDIO_CONSTRAINTS.MAX_VOLUME, volume)
  );
}

/**
 * Convert Float32Array to AudioBuffer
 */
export function createAudioBufferFromFloat32(
  audioContext: AudioContext,
  data: Float32Array,
  format: AudioFormat
): AudioBuffer {
  const numChannels = format.channels;
  const framesPerChannel = data.length / numChannels;

  const buffer = audioContext.createBuffer(
    numChannels,
    framesPerChannel,
    format.sampleRate
  );

  // Copy data to buffer, handling interleaved multi-channel data
  if (numChannels === 1) {
    // Mono: direct copy using getChannelData
    const channelData = buffer.getChannelData(0);
    channelData.set(data);
  } else {
    // Multi-channel: de-interleave
    for (let channel = 0; channel < numChannels; channel++) {
      const channelData = buffer.getChannelData(channel);
      for (let i = 0; i < framesPerChannel; i++) {
        channelData[i] = data[i * numChannels + channel];
      }
    }
  }

  return buffer;
}
