/**
 * useStreamBuffer Hook
 *
 * Manages buffer health monitoring for streaming TTS audio.
 * Tracks buffer states: healthy, low, critical, starved, complete.
 */

import { useState, useCallback, useEffect } from 'react';
import type { BufferStatus, BufferHealth } from '@/types/audio';
import { AUDIO_BUFFER_CONFIG, calculateBufferStatus } from '@/types/audio';

export interface UseStreamBufferOptions {
  /** Target buffer size in seconds */
  targetBufferSize?: number;
  /** Low buffer threshold in seconds */
  lowThreshold?: number;
  /** Critical buffer threshold in seconds */
  criticalThreshold?: number;
  /** Callback when buffer status changes */
  onStatusChange?: (status: BufferStatus) => void;
  /** Callback when buffer becomes critically low */
  onCriticalBuffer?: (bufferedAhead: number) => void;
  /** Callback when buffer is healthy again */
  onBufferHealthy?: (bufferedAhead: number) => void;
}

export interface UseStreamBufferReturn {
  /** Current buffer health */
  bufferHealth: BufferHealth;
  /** Buffer status */
  bufferStatus: BufferStatus;
  /** Seconds buffered ahead */
  bufferedAhead: number;
  /** Total duration buffered */
  totalBuffered: number;
  /** Update buffer metrics */
  updateBufferMetrics: (metrics: {
    bufferedAhead: number;
    totalBuffered: number;
    playheadPosition: number;
    isComplete: boolean;
  }) => void;
  /** Reset buffer state */
  reset: () => void;
  /** Whether buffer needs attention */
  needsAttention: boolean;
  /** Whether buffer is healthy */
  isHealthy: boolean;
}

/**
 * Hook for managing stream buffer health
 */
export function useStreamBuffer(
  options: UseStreamBufferOptions = {}
): UseStreamBufferReturn {
  const {
    targetBufferSize = AUDIO_BUFFER_CONFIG.TARGET_BUFFER_SIZE,
    lowThreshold = AUDIO_BUFFER_CONFIG.LOW_BUFFER_THRESHOLD,
    criticalThreshold = AUDIO_BUFFER_CONFIG.CRITICAL_BUFFER_THRESHOLD,
    onStatusChange,
    onCriticalBuffer,
    onBufferHealthy,
  } = options;

  // State
  const [bufferHealth, setBufferHealth] = useState<BufferHealth>({
    status: 'empty',
    bufferedAhead: 0,
    targetBuffer: targetBufferSize,
    totalBuffered: 0,
    playheadPosition: 0,
  });

  const [previousStatus, setPreviousStatus] = useState<BufferStatus>('empty');

  /**
   * Update buffer metrics and calculate health status
   */
  const updateBufferMetrics = useCallback(
    (metrics: {
      bufferedAhead: number;
      totalBuffered: number;
      playheadPosition: number;
      isComplete: boolean;
    }) => {
      const { bufferedAhead, totalBuffered, playheadPosition, isComplete } = metrics;

      // Calculate buffer status
      const status = calculateBufferStatus(bufferedAhead, isComplete);

      // Update buffer health state
      const newHealth: BufferHealth = {
        status,
        bufferedAhead,
        targetBuffer: targetBufferSize,
        totalBuffered,
        playheadPosition,
      };

      setBufferHealth(newHealth);

      // Emit status change event if changed
      if (status !== previousStatus) {
        setPreviousStatus(status);
        onStatusChange?.(status);

        // Emit specific events
        if (status === 'critical' || status === 'empty') {
          onCriticalBuffer?.(bufferedAhead);
        } else if (status === 'healthy' && (previousStatus === 'critical' || previousStatus === 'low')) {
          onBufferHealthy?.(bufferedAhead);
        }
      }
    },
    [targetBufferSize, previousStatus, onStatusChange, onCriticalBuffer, onBufferHealthy]
  );

  /**
   * Reset buffer state
   */
  const reset = useCallback(() => {
    setBufferHealth({
      status: 'empty',
      bufferedAhead: 0,
      targetBuffer: targetBufferSize,
      totalBuffered: 0,
      playheadPosition: 0,
    });
    setPreviousStatus('empty');
  }, [targetBufferSize]);

  // Derived state
  const needsAttention =
    bufferHealth.status === 'low' ||
    bufferHealth.status === 'critical' ||
    bufferHealth.status === 'empty';

  const isHealthy =
    bufferHealth.status === 'healthy' ||
    bufferHealth.status === 'complete';

  return {
    bufferHealth,
    bufferStatus: bufferHealth.status,
    bufferedAhead: bufferHealth.bufferedAhead,
    totalBuffered: bufferHealth.totalBuffered,
    updateBufferMetrics,
    reset,
    needsAttention,
    isHealthy,
  };
}
