/**
 * useTTS Hook
 *
 * Manages TTS request state and orchestrates text-to-speech synthesis.
 * Handles validation, gRPC communication, and error management.
 */

import { useState, useCallback, useRef, useEffect } from 'react';
import { v4 as uuidv4 } from 'uuid';
import type {
  TTSRequest,
  TTSStatus,
  TTSError,
  VoiceConfig,
} from '@/types/tts';
import { DEFAULT_VOICE_CONFIG, validateTTSText } from '@/types/tts';
import { streamTTSRealtime } from '@/lib/realtime-streaming-client';

export interface UseTTSOptions {
  /** Initial voice configuration */
  voiceConfig?: VoiceConfig;
  /** Auto-play audio when ready */
  autoPlay?: boolean;
  /** Callback when synthesis starts */
  onStart?: (requestId: string) => void;
  /** Callback when each audio chunk is received */
  onChunk?: () => void;
  /** Callback when synthesis completes */
  onComplete?: (requestId: string) => void;
  /** Callback when error occurs */
  onError?: (error: TTSError) => void;
}

export interface UseTTSReturn {
  /** Current TTS request (null if idle) */
  currentRequest: TTSRequest | null;
  /** Current TTS status */
  status: TTSStatus;
  /** Current error (null if no error) */
  error: TTSError | null;
  /** Voice configuration */
  voiceConfig: VoiceConfig;
  /** Start TTS synthesis */
  synthesize: (text: string) => Promise<void>;
  /** Cancel ongoing synthesis */
  cancel: () => void;
  /** Clear error */
  clearError: () => void;
  /** Update voice configuration */
  setVoiceConfig: (config: VoiceConfig) => void;
  /** Retry last failed request */
  retry: () => Promise<void>;
  /** Replay last audio */
  replay: () => void;
  /** Whether synthesis is in progress */
  isSynthesizing: boolean;
  /** Whether ready to synthesize */
  isReady: boolean;
  /** Whether audio can be replayed */
  canReplay: boolean;
  /** Current playback time in seconds */
  currentTime: number;
  /** Total duration in seconds */
  duration: number | null;
}

export function useTTS(options: UseTTSOptions = {}): UseTTSReturn {
  const {
    voiceConfig: initialVoiceConfig = DEFAULT_VOICE_CONFIG,
    autoPlay = true,
    onStart,
    onChunk,
    onComplete,
    onError,
  } = options;

  // State
  const [currentRequest, setCurrentRequest] = useState<TTSRequest | null>(null);
  const [status, setStatus] = useState<TTSStatus>('idle');
  const [error, setError] = useState<TTSError | null>(null);
  const [voiceConfig, setVoiceConfig] = useState<VoiceConfig>(initialVoiceConfig);
  const [currentTime, setCurrentTime] = useState<number>(0);
  const [duration, setDuration] = useState<number | null>(null);

  // Refs for maintaining instances
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const lastRequestTextRef = useRef<string>('');
  const lastCompletedAudioRef = useRef<HTMLAudioElement | null>(null);
  const progressIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const audioListenersRef = useRef<{
    play: () => void;
    ended: () => void;
    error: (e: Event) => void;
  } | null>(null);

  // Initialize audio element on mount
  useEffect(() => {
    // Create HTML audio element for playback (will be replaced by streaming audio)
    if (!audioRef.current) {
      audioRef.current = new Audio();

      // Store listeners so we can remove them later
      audioListenersRef.current = {
        play: () => {
          console.log(`[${new Date().toISOString()}] [useTTS] Playback started`);
          setStatus('playing');
        },
        ended: () => {
          console.log(`[${new Date().toISOString()}] [useTTS] Playback ended`);
          setStatus('completed');
          setCurrentRequest(prev =>
            prev ? { ...prev, status: 'completed', updatedAt: new Date() } : null
          );
        },
        error: (e: Event) => {
          console.error(`[${new Date().toISOString()}] [useTTS] Playback error:`, e);
          const playbackError: TTSError = {
            code: 'PLAYBACK_ERROR',
            message: 'Audio playback failed',
            timestamp: new Date(),
          };
          setError(playbackError);
          setStatus('failed');
        },
      };

      audioRef.current.addEventListener('play', audioListenersRef.current.play);
      audioRef.current.addEventListener('ended', audioListenersRef.current.ended);
      audioRef.current.addEventListener('error', audioListenersRef.current.error);
    }

    // Cleanup on unmount
    return () => {
      if (progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
        progressIntervalRef.current = null;
      }
      if (audioRef.current && audioListenersRef.current) {
        audioRef.current.removeEventListener('play', audioListenersRef.current.play);
        audioRef.current.removeEventListener('ended', audioListenersRef.current.ended);
        audioRef.current.removeEventListener('error', audioListenersRef.current.error);
        audioRef.current.pause();
        audioRef.current.src = '';
        audioRef.current = null;
      }
    };
  }, []);

  // Track progress during playback
  useEffect(() => {
    if (status === 'playing' && audioRef.current) {
      // Clear any existing interval
      if (progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
      }

      // Update progress every 100ms
      progressIntervalRef.current = setInterval(() => {
        if (audioRef.current) {
          setCurrentTime(audioRef.current.currentTime);
          if (audioRef.current.duration && isFinite(audioRef.current.duration)) {
            setDuration(audioRef.current.duration);
          }
        }
      }, 100);
    } else {
      // Clear interval when not playing
      if (progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
        progressIntervalRef.current = null;
      }
    }

    return () => {
      if (progressIntervalRef.current) {
        clearInterval(progressIntervalRef.current);
        progressIntervalRef.current = null;
      }
    };
  }, [status]);

  /**
   * Start TTS synthesis
   */
  const synthesize = useCallback(
    async (text: string) => {
      // Validate text
      const validationErrors = validateTTSText(text);
      if (validationErrors.length > 0) {
        const validationError: TTSError = {
          code: 'VALIDATION_ERROR',
          message: validationErrors.join('; '),
          timestamp: new Date(),
        };
        setError(validationError);
        onError?.(validationError);
        return;
      }

      // Clear any previous error
      setError(null);

      // Create request
      const requestId = uuidv4();
      const request: TTSRequest = {
        id: requestId,
        text,
        voiceConfig,
        status: 'pending',
        createdAt: new Date(),
        updatedAt: new Date(),
      };

      // Store request and text for retry
      setCurrentRequest(request);
      lastRequestTextRef.current = text;
      setStatus('pending');

      try {
        // Call start callback
        onStart?.(requestId);

        // Stream TTS audio from API with real-time playback
        console.log(`[${new Date().toISOString()}] [useTTS] Starting real-time TTS stream for request:`, requestId);
        setStatus('streaming');

        const audio = await streamTTSRealtime(
          {
            text,
            language: voiceConfig.language,
            voice: voiceConfig.voice,
            speed: voiceConfig.speed,
          },
          {
            onStart: () => {
              console.log(`[${new Date().toISOString()}] [useTTS] Stream started`);
            },
            onFirstChunk: () => {
              console.log(`[${new Date().toISOString()}] [useTTS] First chunk received, playback starting`);
              setStatus('playing');
            },
            onChunk: () => {
              // Pass through chunk callback
              onChunk?.();
            },
            onProgress: (bytesReceived) => {
              console.log(`[${new Date().toISOString()}] [useTTS] Bytes received:`, bytesReceived);
            },
            onComplete: () => {
              console.log(`[${new Date().toISOString()}] [useTTS] Stream completed`);
            },
            onError: (streamError) => {
              console.error(`[${new Date().toISOString()}] [useTTS] Stream error:`, streamError);
              const ttsError: TTSError = {
                code: 'TTS_STREAM_ERROR',
                message: streamError.message || 'An error occurred during synthesis',
                timestamp: new Date(),
                details: {
                  requestId,
                  originalError: streamError.toString(),
                },
              };
              setError(ttsError);
              setStatus('failed');
              setCurrentRequest(prev =>
                prev ? { ...prev, status: 'failed', error: ttsError, updatedAt: new Date() } : null
              );
              onError?.(ttsError);
            },
          }
        );

        // Replace the existing audio element with the new streaming one
        if (audioRef.current && audioListenersRef.current) {
          // Remove all event listeners from the old audio element before replacing
          const oldAudio = audioRef.current;
          oldAudio.removeEventListener('play', audioListenersRef.current.play);
          oldAudio.removeEventListener('ended', audioListenersRef.current.ended);
          oldAudio.removeEventListener('error', audioListenersRef.current.error);
          oldAudio.pause();
          oldAudio.src = '';
        }
        audioRef.current = audio;

        // Add event listeners to the new audio element
        audio.addEventListener('play', () => {
          console.log(`[${new Date().toISOString()}] [useTTS] Playback started`);
          setStatus('playing');
        });
        audio.addEventListener('ended', () => {
          console.log(`[${new Date().toISOString()}] [useTTS] Playback ended`);
          setStatus('completed');
          setCurrentRequest(prev =>
            prev ? { ...prev, status: 'completed', updatedAt: new Date() } : null
          );
          // Store the completed audio for replay
          lastCompletedAudioRef.current = audio;
          // Update final time
          if (audio.duration && isFinite(audio.duration)) {
            setCurrentTime(audio.duration);
            setDuration(audio.duration);
          }
          onComplete?.(requestId);
        });
        audio.addEventListener('error', (e) => {
          console.error(`[${new Date().toISOString()}] [useTTS] Playback error:`, e);
          const playbackError: TTSError = {
            code: 'PLAYBACK_ERROR',
            message: 'Audio playback failed',
            timestamp: new Date(),
          };
          setError(playbackError);
          setStatus('failed');
        });

      } catch (err) {
        console.error(`[${new Date().toISOString()}] [useTTS] Synthesis error:`, err);
        const synthesisError: TTSError = {
          code: 'GRPC_CONNECTION_FAILED',
          message: err instanceof Error ? err.message : 'Failed to connect to TTS service',
          timestamp: new Date(),
          details: {
            requestId,
            error: err instanceof Error ? err.stack : String(err),
          },
        };
        setError(synthesisError);
        setStatus('failed');
        setCurrentRequest(prev =>
          prev ? { ...prev, status: 'failed', error: synthesisError, updatedAt: new Date() } : null
        );
        onError?.(synthesisError);
      }
    },
    [voiceConfig, autoPlay, onStart, onChunk, onComplete, onError]
  );

  /**
   * Cancel ongoing synthesis
   */
  const cancel = useCallback(() => {
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current.src = '';
    }
    setStatus('cancelled');
    setCurrentRequest(prev =>
      prev ? { ...prev, status: 'cancelled', updatedAt: new Date() } : null
    );
  }, []);

  /**
   * Clear error
   */
  const clearError = useCallback(() => {
    setError(null);
    if (status === 'failed') {
      setStatus('idle');
    }
  }, [status]);

  /**
   * Retry last failed request
   */
  const retry = useCallback(async () => {
    if (lastRequestTextRef.current) {
      await synthesize(lastRequestTextRef.current);
    }
  }, [synthesize]);

  /**
   * Replay last completed audio
   */
  const replay = useCallback(() => {
    if (lastCompletedAudioRef.current) {
      const audio = lastCompletedAudioRef.current;
      audio.currentTime = 0;
      setCurrentTime(0);
      setStatus('playing');
      audio.play().catch((err) => {
        console.error('Replay failed:', err);
        const replayError: TTSError = {
          code: 'PLAYBACK_ERROR',
          message: 'Failed to replay audio',
          timestamp: new Date(),
        };
        setError(replayError);
        setStatus('failed');
      });
    }
  }, []);

  // Derived state
  const isSynthesizing =
    status === 'pending' || status === 'streaming' || status === 'playing';
  const isReady = status === 'idle' || status === 'completed' || status === 'failed';
  const canReplay = lastCompletedAudioRef.current !== null && (status === 'completed' || status === 'idle');

  return {
    currentRequest,
    status,
    error,
    voiceConfig,
    synthesize,
    cancel,
    clearError,
    setVoiceConfig,
    retry,
    replay,
    isSynthesizing,
    isReady,
    canReplay,
    currentTime,
    duration,
  };
}
