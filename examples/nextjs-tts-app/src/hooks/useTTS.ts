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
import { TTSGRPCClient } from '@/lib/grpc-client';
import { StreamHandler, StreamHandlerEvent } from '@/lib/stream-handler';
import { AudioPlayer } from '@/lib/audio-player';

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
  const grpcClientRef = useRef<TTSGRPCClient | null>(null);
  const audioPlayerRef = useRef<AudioPlayer | null>(null);
  const streamHandlerRef = useRef<StreamHandler | null>(null);
  const lastRequestTextRef = useRef<string>('');

  // Initialize gRPC client and audio player on mount
  useEffect(() => {
    // Create gRPC client
    if (!grpcClientRef.current) {
      grpcClientRef.current = new TTSGRPCClient();
    }

    // Create audio player
    if (!audioPlayerRef.current) {
      audioPlayerRef.current = new AudioPlayer();
    }

    // Create stream handler
    if (!streamHandlerRef.current && grpcClientRef.current && audioPlayerRef.current) {
      streamHandlerRef.current = new StreamHandler(
        grpcClientRef.current,
        audioPlayerRef.current
      );

      // Set up event listeners
      streamHandlerRef.current.on(StreamHandlerEvent.Ready, ({ requestId }) => {
        console.log('[useTTS] Stream ready:', requestId);
        setStatus('streaming');
      });

      streamHandlerRef.current.on(StreamHandlerEvent.ChunkReceived, (data) => {
        console.log('[useTTS] Chunk received:', data.sequenceNumber);
        // Update status is already streaming
      });

      streamHandlerRef.current.on(StreamHandlerEvent.PlaybackStarted, ({ requestId }) => {
        console.log('[useTTS] Playback started:', requestId);
        setStatus('playing');
      });

      streamHandlerRef.current.on(StreamHandlerEvent.Completed, ({ requestId }) => {
        console.log('[useTTS] Stream completed:', requestId);
        setStatus('completed');
        setCurrentRequest(prev => prev ? { ...prev, status: 'completed', updatedAt: new Date() } : null);
        onComplete?.(requestId);
      });

      streamHandlerRef.current.on(StreamHandlerEvent.Error, ({ requestId, error: streamError }) => {
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
      });

      streamHandlerRef.current.on(StreamHandlerEvent.Cancelled, ({ requestId }) => {
        console.log('[useTTS] Stream cancelled:', requestId);
        setStatus('cancelled');
        setCurrentRequest(prev =>
          prev ? { ...prev, status: 'cancelled', updatedAt: new Date() } : null
        );
      });
    }

    // Cleanup on unmount
    return () => {
      if (streamHandlerRef.current) {
        streamHandlerRef.current.reset();
      }
      if (audioPlayerRef.current) {
        audioPlayerRef.current.dispose();
      }
      if (grpcClientRef.current) {
        grpcClientRef.current.disconnect();
      }
    };
  }, [onComplete, onError]);

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

        // Connect to gRPC service if not connected
        if (grpcClientRef.current && !grpcClientRef.current.isConnected()) {
          await grpcClientRef.current.connect();
        }

        // Start streaming
        if (streamHandlerRef.current) {
          await streamHandlerRef.current.start(request, autoPlay);
        } else {
          throw new Error('Stream handler not initialized');
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
    [voiceConfig, autoPlay, onStart, onError]
  );

  /**
   * Cancel ongoing synthesis
   */
  const cancel = useCallback(() => {
    if (streamHandlerRef.current) {
      streamHandlerRef.current.cancel();
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
