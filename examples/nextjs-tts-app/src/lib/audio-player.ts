/**
 * Web Audio API Wrapper
 *
 * Manages audio playback using the Web Audio API for streaming TTS audio.
 * Handles buffering, playback control, and audio chunk scheduling.
 */

import type {
  AudioChunk,
  AudioFormat,
  PlaybackState,
  BufferStatus,
  PlaybackControl,
  BufferHealth,
  AudioPlayerEventData,
} from '@/types/audio';
import {
  DEFAULT_AUDIO_FORMAT,
  AUDIO_BUFFER_CONFIG,
  AUDIO_CONSTRAINTS,
  calculateBufferStatus,
  validateVolume,
  AudioPlayerEvent,
} from '@/types/audio';

/**
 * Event listener type
 */
type EventListener<E extends keyof AudioPlayerEventData> = (data: AudioPlayerEventData[E]) => void;

/**
 * Web Audio Player for streaming TTS
 *
 * Manages audio playback with buffering, scheduling, and playback control.
 */
export class AudioPlayer {
  private audioContext: AudioContext | null = null;
  private gainNode: GainNode | null = null;
  private chunks: AudioChunk[] = [];
  private scheduledBuffers: AudioBufferSourceNode[] = [];
  private requestId: string | null = null;
  private format: AudioFormat = DEFAULT_AUDIO_FORMAT;
  private playbackStartTime: number = 0;
  private pausedAt: number = 0;
  private isStreamComplete: boolean = false;

  // State
  private _state: PlaybackState = 'idle';
  private _volume: number = AUDIO_CONSTRAINTS.DEFAULT_VOLUME;
  private _muted: boolean = false;
  private _currentTime: number = 0;

  // Event listeners
  private listeners: Map<keyof AudioPlayerEventData, Set<EventListener<any>>> = new Map();

  constructor() {
    // Audio context will be initialized on first play (requires user interaction)
  }

  /**
   * Initialize audio context (must be called after user interaction)
   */
  private async initAudioContext(): Promise<void> {
    if (this.audioContext) {
      return;
    }

    this.audioContext = new AudioContext({
      sampleRate: this.format.sampleRate,
    });

    // Create gain node for volume control
    this.gainNode = this.audioContext.createGain();
    this.gainNode.connect(this.audioContext.destination);
    this.gainNode.gain.value = this._muted ? 0 : this._volume;

    console.log('[AudioPlayer] Initialized AudioContext:', {
      sampleRate: this.audioContext.sampleRate,
      state: this.audioContext.state,
    });
  }

  /**
   * Add an audio chunk to the buffer
   */
  addChunk(chunk: AudioChunk): void {
    this.chunks.push(chunk);

    this.emit('chunk-received', {
      requestId: this.requestId!,
      sequenceNumber: chunk.sequenceNumber,
      duration: chunk.duration,
    });

    // Check if we have enough buffer to start playback
    const bufferHealth = this.getBufferHealth();
    if (
      this._state === 'buffering' &&
      bufferHealth.bufferedAhead >= AUDIO_BUFFER_CONFIG.MIN_BUFFER_BEFORE_PLAYBACK
    ) {
      console.log('[AudioPlayer] Buffer ready, starting playback');
      this.scheduleNextChunks();
      this._state = 'playing';
      this.emit('playback-started', { requestId: this.requestId! });
    }

    // Update buffer health status
    this.checkBufferHealth(bufferHealth);
  }

  /**
   * Start playing audio (or resume if paused)
   */
  async play(requestId: string, format: AudioFormat = DEFAULT_AUDIO_FORMAT): Promise<void> {
    this.requestId = requestId;
    this.format = format;

    await this.initAudioContext();

    if (this._state === 'paused') {
      // Resume from pause
      await this.audioContext!.resume();
      this.playbackStartTime = this.audioContext!.currentTime - this.pausedAt;
      this._state = 'playing';
      this.emit('playback-resumed', { requestId, currentTime: this.pausedAt });
      this.scheduleNextChunks();
      return;
    }

    if (this._state === 'playing') {
      console.warn('[AudioPlayer] Already playing');
      return;
    }

    // Start new playback
    this._state = 'buffering';
    this.playbackStartTime = this.audioContext!.currentTime;
    this.pausedAt = 0;
    this.isStreamComplete = false;

    console.log('[AudioPlayer] Starting playback (buffering)');

    // If we already have enough chunks, start playing immediately
    const bufferHealth = this.getBufferHealth();
    if (bufferHealth.bufferedAhead >= AUDIO_BUFFER_CONFIG.MIN_BUFFER_BEFORE_PLAYBACK) {
      this.scheduleNextChunks();
      this._state = 'playing';
      this.emit('playback-started', { requestId });
    }
  }

  /**
   * Pause playback
   */
  async pause(): Promise<void> {
    if (this._state !== 'playing' && this._state !== 'buffering') {
      return;
    }

    await this.audioContext?.suspend();
    this.pausedAt = this.audioContext!.currentTime - this.playbackStartTime;
    this._state = 'paused';

    this.emit('playback-paused', {
      requestId: this.requestId!,
      currentTime: this.pausedAt,
    });

    console.log('[AudioPlayer] Paused at', this.pausedAt);
  }

  /**
   * Stop playback and clear buffer
   */
  stop(): void {
    if (this._state === 'idle' || this._state === 'stopped') {
      return;
    }

    // Stop all scheduled buffers
    this.scheduledBuffers.forEach(buffer => {
      try {
        buffer.stop();
      } catch (e) {
        // Ignore errors from already stopped buffers
      }
    });

    this.scheduledBuffers = [];
    this._state = 'stopped';

    this.emit('playback-stopped', { requestId: this.requestId! });

    console.log('[AudioPlayer] Stopped');
  }

  /**
   * Mark stream as complete (no more chunks coming)
   */
  markStreamComplete(): void {
    this.isStreamComplete = true;
    console.log('[AudioPlayer] Stream marked as complete');
  }

  /**
   * Schedule next audio chunks for playback
   */
  private scheduleNextChunks(): void {
    if (!this.audioContext || !this.gainNode) {
      return;
    }

    const currentTime = this.audioContext.currentTime;
    const scheduledUntil = this.playbackStartTime + this.getTotalScheduledDuration();

    // Schedule chunks that haven't been scheduled yet
    let nextScheduleTime = scheduledUntil;

    for (const chunk of this.chunks) {
      // Check if this chunk is already scheduled
      const alreadyScheduled = this.scheduledBuffers.some(
        buffer => (buffer as any)._chunkSequence === chunk.sequenceNumber
      );

      if (alreadyScheduled) {
        continue;
      }

      // Create audio buffer
      const audioBuffer = this.createAudioBuffer(chunk.data);
      const source = this.audioContext.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(this.gainNode);

      // Store chunk sequence for tracking
      (source as any)._chunkSequence = chunk.sequenceNumber;

      // Schedule playback
      const scheduleTime = Math.max(currentTime, nextScheduleTime);
      source.start(scheduleTime);

      // Handle completion
      source.onended = () => {
        this.scheduledBuffers = this.scheduledBuffers.filter(b => b !== source);

        // Check if all chunks have finished and stream is complete
        if (
          this.isStreamComplete &&
          this.scheduledBuffers.length === 0 &&
          this._state === 'playing'
        ) {
          this._state = 'idle';
          this.emit('playback-ended', {
            requestId: this.requestId!,
            duration: this.getTotalDuration(),
          });
        }
      };

      this.scheduledBuffers.push(source);
      nextScheduleTime = scheduleTime + audioBuffer.duration;

      console.log('[AudioPlayer] Scheduled chunk', chunk.sequenceNumber, 'at', scheduleTime);
    }
  }

  /**
   * Create AudioBuffer from Float32Array
   */
  private createAudioBuffer(data: Float32Array): AudioBuffer {
    const numChannels = this.format.channels;
    const framesPerChannel = data.length / numChannels;

    const buffer = this.audioContext!.createBuffer(
      numChannels,
      framesPerChannel,
      this.format.sampleRate
    );

    // Copy data to buffer, handling interleaved multi-channel data
    if (numChannels === 1) {
      // Mono: direct copy
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

  /**
   * Get total duration of all chunks
   */
  private getTotalDuration(): number {
    return this.chunks.reduce((sum, chunk) => sum + chunk.duration, 0);
  }

  /**
   * Get total duration of scheduled chunks
   */
  private getTotalScheduledDuration(): number {
    return this.chunks
      .filter(chunk =>
        this.scheduledBuffers.some(
          buffer => (buffer as any)._chunkSequence === chunk.sequenceNumber
        )
      )
      .reduce((sum, chunk) => sum + chunk.duration, 0);
  }

  /**
   * Get current playback position
   */
  getCurrentTime(): number {
    if (!this.audioContext) {
      return 0;
    }

    if (this._state === 'paused') {
      return this.pausedAt;
    }

    if (this._state === 'playing' || this._state === 'buffering') {
      return this.audioContext.currentTime - this.playbackStartTime;
    }

    return 0;
  }

  /**
   * Get buffer health metrics
   */
  getBufferHealth(): BufferHealth {
    const currentTime = this.getCurrentTime();
    const totalBuffered = this.getTotalDuration();
    const bufferedAhead = Math.max(0, totalBuffered - currentTime);
    const status = calculateBufferStatus(bufferedAhead, this.isStreamComplete);

    return {
      status,
      bufferedAhead,
      targetBuffer: AUDIO_BUFFER_CONFIG.TARGET_BUFFER_SIZE,
      totalBuffered,
      playheadPosition: currentTime,
    };
  }

  /**
   * Check buffer health and emit events if needed
   */
  private checkBufferHealth(health: BufferHealth): void {
    if (health.status === 'critical') {
      this.emit('buffer-critical', {
        requestId: this.requestId!,
        bufferedAhead: health.bufferedAhead,
      });
    } else if (health.status === 'low') {
      this.emit('buffer-low', {
        requestId: this.requestId!,
        bufferedAhead: health.bufferedAhead,
      });
    } else if (health.status === 'healthy') {
      this.emit('buffer-healthy', {
        requestId: this.requestId!,
        bufferedAhead: health.bufferedAhead,
      });
    }
  }

  /**
   * Get playback control state
   */
  getPlaybackControl(): PlaybackControl {
    return {
      state: this._state,
      volume: this._volume,
      muted: this._muted,
      currentTime: this.getCurrentTime(),
      duration: this.isStreamComplete ? this.getTotalDuration() : null,
    };
  }

  /**
   * Set volume (0.0 to 1.0)
   */
  setVolume(volume: number): void {
    this._volume = validateVolume(volume);
    if (this.gainNode && !this._muted) {
      this.gainNode.gain.value = this._volume;
    }
    this.emit('volume-changed', { volume: this._volume, muted: this._muted });
  }

  /**
   * Set mute state
   */
  setMuted(muted: boolean): void {
    this._muted = muted;
    if (this.gainNode) {
      this.gainNode.gain.value = muted ? 0 : this._volume;
    }
    this.emit('volume-changed', { volume: this._volume, muted: this._muted });
  }

  /**
   * Clear all chunks and reset state
   */
  reset(): void {
    this.stop();
    this.chunks = [];
    this.scheduledBuffers = [];
    this.requestId = null;
    this.playbackStartTime = 0;
    this.pausedAt = 0;
    this.isStreamComplete = false;
    this._state = 'idle';
    console.log('[AudioPlayer] Reset');
  }

  /**
   * Dispose of audio context and resources
   */
  async dispose(): Promise<void> {
    this.reset();
    if (this.audioContext) {
      await this.audioContext.close();
      this.audioContext = null;
      this.gainNode = null;
    }
    this.listeners.clear();
    console.log('[AudioPlayer] Disposed');
  }

  // ============================================================================
  // Event Management
  // ============================================================================

  /**
   * Add event listener
   */
  on<E extends keyof AudioPlayerEventData>(event: E, listener: EventListener<E>): void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set());
    }
    this.listeners.get(event)!.add(listener);
  }

  /**
   * Remove event listener
   */
  off<E extends keyof AudioPlayerEventData>(event: E, listener: EventListener<E>): void {
    const listeners = this.listeners.get(event);
    if (listeners) {
      listeners.delete(listener);
    }
  }

  /**
   * Emit event
   */
  private emit<E extends keyof AudioPlayerEventData>(event: E, data: AudioPlayerEventData[E]): void {
    const listeners = this.listeners.get(event);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(data);
        } catch (e) {
          console.error('[AudioPlayer] Error in event listener:', e);
        }
      });
    }
  }
}
