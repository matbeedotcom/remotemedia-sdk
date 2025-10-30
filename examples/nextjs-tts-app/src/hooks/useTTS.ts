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
import { streamTTS } from '@/lib/streaming-api-client';

export interface UseTTSOptions {
  /** Initial voice configuration */
  voiceConfig?: VoiceConfig;
  /** Auto-play audio when ready */
  autoPlay?: boolean;
  /** Callback when synthesis starts */
  onStart?: (requestId: string) => void;
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
  /** Whether synthesis is in progress */
  isSynthesizing: boolean;
  /** Whether ready to synthesize */
  isReady: boolean;
}

export function useTTS(options: UseTTSOptions = {}): UseTTSReturn {
  const {
    voiceConfig: initialVoiceConfig = DEFAULT_VOICE_CONFIG,
    autoPlay = true,
    onStart,
    onComplete,
    onError,
  } = options;

  // State
  const [currentRequest, setCurrentRequest] = useState<TTSRequest | null>(null);
  const [status, setStatus] = useState<TTSStatus>('idle');
  const [error, setError] = useState<TTSError | null>(null);
  const [voiceConfig, setVoiceConfig] = useState<VoiceConfig>(initialVoiceConfig);

  // Refs for maintaining instances
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const lastRequestTextRef = useRef<string>('');

  // Initialize audio element on mount
  useEffect(() => {
    // Create HTML audio element for playback
    if (!audioRef.current) {
      audioRef.current = new Audio();
      audioRef.current.addEventListener('play', () => {
        console.log('[useTTS] Playback started');
        setStatus('playing');
      });
      audioRef.current.addEventListener('ended', () => {
        console.log('[useTTS] Playback ended');
        setStatus('completed');
        setCurrentRequest(prev =>
          prev ? { ...prev, status: 'completed', updatedAt: new Date() } : null
        );
      });
      audioRef.current.addEventListener('error', (e) => {
        console.error('[useTTS] Playback error:', e);
        const playbackError: TTSError = {
          code: 'PLAYBACK_ERROR',
          message: 'Audio playback failed',
          timestamp: new Date(),
        };
        setError(playbackError);
        setStatus('failed');
      });
    }

    // Cleanup on unmount
    return () => {
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current.src = '';
        audioRef.current = null;
      }
    };
  }, []);

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

        // Stream TTS audio from API
        console.log('[useTTS] Starting TTS stream for request:', requestId);
        setStatus('streaming');

        const audioUrl = await streamTTS(
          {
            text,
            language: voiceConfig.language,
            voice: voiceConfig.voice,
            speed: voiceConfig.speed,
          },
          {
            onStart: () => {
              console.log('[useTTS] Stream started');
            },
            onProgress: (bytesReceived) => {
              console.log('[useTTS] Bytes received:', bytesReceived);
            },
            onComplete: () => {
              console.log('[useTTS] Stream completed');
              // Audio will start playing automatically via audio element
            },
            onError: (streamError) => {
              console.error('[useTTS] Stream error:', streamError);
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

        // Set audio source and play if autoPlay is enabled
        if (audioRef.current) {
          audioRef.current.src = audioUrl;
          if (autoPlay) {
            await audioRef.current.play();
          }
        }

      } catch (err) {
        console.error('[useTTS] Synthesis error:', err);
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
    [voiceConfig, autoPlay, onStart, onComplete, onError]
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

  // Derived state
  const isSynthesizing =
    status === 'pending' || status === 'streaming' || status === 'playing';
  const isReady = status === 'idle' || status === 'completed' || status === 'failed';

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
    isSynthesizing,
    isReady,
  };
}
