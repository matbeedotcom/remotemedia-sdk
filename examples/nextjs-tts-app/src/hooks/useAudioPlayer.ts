/**
 * useAudioPlayer Hook
 *
 * Manages Web Audio API playback and integrates with AudioPlayer class.
 * Provides React state management for audio playback controls.
 */

import { useState, useCallback, useEffect, useRef } from 'react';
import type {
  PlaybackState,
  BufferStatus,
  PlaybackControl,
  BufferHealth,
} from '@/types/audio';
import { AudioPlayer } from '@/lib/audio-player';

export interface UseAudioPlayerReturn {
  /** Audio player instance */
  audioPlayer: AudioPlayer | null;
  /** Current playback state */
  playbackState: PlaybackState;
  /** Current playback time in seconds */
  currentTime: number;
  /** Total audio duration in seconds (null if unknown) */
  duration: number | null;
  /** Current volume (0.0 to 1.0) */
  volume: number;
  /** Whether audio is muted */
  muted: boolean;
  /** Buffer health status */
  bufferStatus: BufferStatus;
  /** Seconds of audio buffered ahead */
  bufferedAhead: number;
  /** Total buffered duration */
  totalBuffered: number;
  /** Play audio */
  play: () => Promise<void>;
  /** Pause audio */
  pause: () => Promise<void>;
  /** Stop audio */
  stop: () => void;
  /** Set volume */
  setVolume: (volume: number) => void;
  /** Toggle mute */
  toggleMute: () => void;
  /** Reset player */
  reset: () => void;
}

/**
 * Hook for managing audio playback
 */
export function useAudioPlayer(): UseAudioPlayerReturn {
  // Audio player instance ref
  const audioPlayerRef = useRef<AudioPlayer | null>(null);

  // State for playback control
  const [playbackState, setPlaybackState] = useState<PlaybackState>('idle');
  const [currentTime, setCurrentTime] = useState<number>(0);
  const [duration, setDuration] = useState<number | null>(null);
  const [volume, setVolumeState] = useState<number>(0.8);
  const [muted, setMuted] = useState<boolean>(false);

  // State for buffer health
  const [bufferStatus, setBufferStatus] = useState<BufferStatus>('empty');
  const [bufferedAhead, setBufferedAhead] = useState<number>(0);
  const [totalBuffered, setTotalBuffered] = useState<number>(0);

  // Interval ref for updating current time
  const updateIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // Initialize audio player on mount
  useEffect(() => {
    if (!audioPlayerRef.current) {
      const player = new AudioPlayer();
      audioPlayerRef.current = player;

      // Set up event listeners
      player.on('playback-started', () => {
        setPlaybackState('playing');
      });

      player.on('playback-paused', ({ currentTime: pauseTime }) => {
        setPlaybackState('paused');
        setCurrentTime(pauseTime);
      });

      player.on('playback-stopped', () => {
        setPlaybackState('stopped');
        setCurrentTime(0);
      });

      player.on('playback-resumed', () => {
        setPlaybackState('playing');
      });

      player.on('playback-ended', ({ duration: endDuration }) => {
        setPlaybackState('idle');
        setCurrentTime(endDuration);
        setDuration(endDuration);
      });

      player.on('buffer-low', ({ bufferedAhead: ahead }) => {
        setBufferStatus('low');
        setBufferedAhead(ahead);
      });

      player.on('buffer-critical', ({ bufferedAhead: ahead }) => {
        setBufferStatus('critical');
        setBufferedAhead(ahead);
      });

      player.on('buffer-healthy', ({ bufferedAhead: ahead }) => {
        setBufferStatus('healthy');
        setBufferedAhead(ahead);
      });

      player.on('chunk-received', () => {
        // Update buffer health when new chunks arrive
        updateBufferHealth();
      });

      player.on('stream-completed', ({ totalDuration }) => {
        setDuration(totalDuration);
      });

      player.on('volume-changed', ({ volume: newVolume, muted: newMuted }) => {
        setVolumeState(newVolume);
        setMuted(newMuted);
      });

      // Start interval for updating current time
      updateIntervalRef.current = setInterval(() => {
        if (player) {
          const control = player.getPlaybackControl();
          if (control.state === 'playing') {
            setCurrentTime(player.getCurrentTime());
          }
          updateBufferHealth();
        }
      }, 100); // Update every 100ms
    }

    // Cleanup on unmount
    return () => {
      if (updateIntervalRef.current) {
        clearInterval(updateIntervalRef.current);
        updateIntervalRef.current = null;
      }
      if (audioPlayerRef.current) {
        audioPlayerRef.current.dispose();
        audioPlayerRef.current = null;
      }
    };
  }, []);

  /**
   * Update buffer health metrics
   */
  const updateBufferHealth = useCallback(() => {
    if (audioPlayerRef.current) {
      const health = audioPlayerRef.current.getBufferHealth();
      setBufferStatus(health.status);
      setBufferedAhead(health.bufferedAhead);
      setTotalBuffered(health.totalBuffered);
    }
  }, []);

  /**
   * Play audio
   */
  const play = useCallback(async () => {
    if (audioPlayerRef.current) {
      // Audio player's play method is called from useTTS hook
      // This is just for resuming from pause
      if (playbackState === 'paused') {
        await audioPlayerRef.current.play('', undefined); // Resume with same request
      }
    }
  }, [playbackState]);

  /**
   * Pause audio
   */
  const pause = useCallback(async () => {
    if (audioPlayerRef.current) {
      await audioPlayerRef.current.pause();
    }
  }, []);

  /**
   * Stop audio
   */
  const stop = useCallback(() => {
    if (audioPlayerRef.current) {
      audioPlayerRef.current.stop();
    }
  }, []);

  /**
   * Set volume
   */
  const setVolume = useCallback((newVolume: number) => {
    if (audioPlayerRef.current) {
      audioPlayerRef.current.setVolume(newVolume);
    }
  }, []);

  /**
   * Toggle mute
   */
  const toggleMute = useCallback(() => {
    if (audioPlayerRef.current) {
      audioPlayerRef.current.setMuted(!muted);
    }
  }, [muted]);

  /**
   * Reset player
   */
  const reset = useCallback(() => {
    if (audioPlayerRef.current) {
      audioPlayerRef.current.reset();
    }
    setPlaybackState('idle');
    setCurrentTime(0);
    setDuration(null);
    setBufferStatus('empty');
    setBufferedAhead(0);
    setTotalBuffered(0);
  }, []);

  return {
    audioPlayer: audioPlayerRef.current,
    playbackState,
    currentTime,
    duration,
    volume,
    muted,
    bufferStatus,
    bufferedAhead,
    totalBuffered,
    play,
    pause,
    stop,
    setVolume,
    toggleMute,
    reset,
  };
}
