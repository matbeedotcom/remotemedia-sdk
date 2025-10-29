/**
 * Audio Type Definitions
 * Based on data-model.md specification
 */

/**
 * Playback state values
 */
export enum PlaybackState {
  /** Audio is not playing */
  Idle = 'idle',
  /** Audio is currently playing */
  Playing = 'playing',
  /** Audio is paused */
  Paused = 'paused',
  /** Audio playback stopped */
  Stopped = 'stopped',
  /** Audio is buffering */
  Buffering = 'buffering',
}

/**
 * Buffer health status
 */
export enum BufferStatus {
  /** Buffer is healthy (>2s ahead) */
  Healthy = 'healthy',
  /** Buffer is low (1-2s ahead) */
  Low = 'low',
  /** Buffer is critically low (<1s ahead) */
  Critical = 'critical',
  /** Buffer is empty */
  Empty = 'empty',
  /** Buffer is full (no more data expected) */
  Complete = 'complete',
}

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
export enum AudioPlayerEvent {
  /** Playback started */
  PlaybackStarted = 'playback-started',
  /** Playback paused */
  PlaybackPaused = 'playback-paused',
  /** Playback stopped */
  PlaybackStopped = 'playback-stopped',
  /** Playback resumed */
  PlaybackResumed = 'playback-resumed',
  /** Playback completed (reached end) */
  PlaybackEnded = 'playback-ended',
  /** Buffer is low */
  BufferLow = 'buffer-low',
  /** Buffer is critically low */
  BufferCritical = 'buffer-critical',
  /** Buffer is healthy again */
  BufferHealthy = 'buffer-healthy',
  /** New audio chunk received */
  ChunkReceived = 'chunk-received',
  /** Audio stream completed */
  StreamCompleted = 'stream-completed',
  /** Audio player error */
  Error = 'error',
  /** Volume changed */
  VolumeChanged = 'volume-changed',
  /** Seek position changed */
  Seeked = 'seeked',
}

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
  return [PlaybackState.Playing, PlaybackState.Buffering].includes(state);
}

/**
 * Type guard to check if buffer needs attention
 */
export function needsBuffering(status: BufferStatus): boolean {
  return [BufferStatus.Low, BufferStatus.Critical, BufferStatus.Empty].includes(status);
}

/**
 * Calculate buffer health status from metrics
 */
export function calculateBufferStatus(bufferedAhead: number, isComplete: boolean): BufferStatus {
  if (isComplete) {
    return BufferStatus.Complete;
  }
  if (bufferedAhead === 0) {
    return BufferStatus.Empty;
  }
  if (bufferedAhead < AUDIO_BUFFER_CONFIG.CRITICAL_BUFFER_THRESHOLD) {
    return BufferStatus.Critical;
  }
  if (bufferedAhead < AUDIO_BUFFER_CONFIG.LOW_BUFFER_THRESHOLD) {
    return BufferStatus.Low;
  }
  return BufferStatus.Healthy;
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
  const buffer = audioContext.createBuffer(
    format.channels,
    data.length / format.channels,
    format.sampleRate
  );

  // Copy data to buffer (mono channel)
  buffer.copyToChannel(data, 0);

  return buffer;
}
