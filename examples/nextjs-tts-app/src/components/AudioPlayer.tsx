'use client';

/**
 * AudioPlayer Component
 *
 * Minimal MVP audio player with play/pause/stop controls.
 * Displays playback status and basic progress information.
 */

import { PlaybackState, BufferStatus } from '@/types/audio';

export interface AudioPlayerProps {
  /** Current playback state */
  playbackState: PlaybackState;
  /** Current playback time in seconds */
  currentTime: number;
  /** Total duration in seconds (null if unknown) */
  duration: number | null;
  /** Buffer health status */
  bufferStatus: BufferStatus;
  /** Buffered ahead duration in seconds */
  bufferedAhead: number;
  /** Callback when pause button is clicked */
  onPause?: () => void;
  /** Callback when play/resume button is clicked */
  onPlay?: () => void;
  /** Callback when stop button is clicked */
  onStop?: () => void;
  /** Custom class name for styling */
  className?: string;
}

/**
 * Format seconds to MM:SS format
 */
function formatTime(seconds: number): string {
  if (!isFinite(seconds)) {
    return '0:00';
  }

  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, '0')}`;
}

/**
 * Get buffer status indicator color
 */
function getBufferColor(status: BufferStatus): string {
  switch (status) {
    case 'healthy':
    case 'complete':
      return 'bg-green-500';
    case 'low':
      return 'bg-yellow-500';
    case 'critical':
      return 'bg-orange-500';
    case 'empty':
      return 'bg-red-500';
    default:
      return 'bg-gray-400';
  }
}

/**
 * Get playback state label
 */
function getStateLabel(state: PlaybackState): string {
  switch (state) {
    case 'playing':
      return 'Playing';
    case 'paused':
      return 'Paused';
    case 'stopped':
      return 'Stopped';
    case 'buffering':
      return 'Buffering...';
    case 'idle':
    default:
      return 'Ready';
  }
}

export function AudioPlayer({
  playbackState,
  currentTime,
  duration,
  bufferStatus,
  bufferedAhead,
  onPause,
  onPlay,
  onStop,
  className = '',
}: AudioPlayerProps) {
  const isPlaying = playbackState === 'playing';
  const isPaused = playbackState === 'paused';
  const isBuffering = playbackState === 'buffering';
  const isIdle = playbackState === 'idle' || playbackState === 'stopped';

  const canPlay = (isIdle || isPaused) && onPlay;
  const canPause = isPlaying && onPause;
  const canStop = !isIdle && onStop;

  // Calculate progress percentage
  const progressPercentage = duration && duration > 0
    ? (currentTime / duration) * 100
    : 0;

  return (
    <div
      className={`
        p-4 rounded-lg
        bg-gray-50 border border-gray-200
        ${className}
      `}
    >
      {/* Status Bar */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          {/* Playback state icon */}
          <div className="text-gray-700">
            {isPlaying && (
              <svg
                className="h-5 w-5 animate-pulse"
                fill="currentColor"
                viewBox="0 0 24 24"
              >
                <path d="M8 5v14l11-7z" />
              </svg>
            )}
            {isPaused && (
              <svg className="h-5 w-5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z" />
              </svg>
            )}
            {isBuffering && (
              <svg
                className="h-5 w-5 animate-spin"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <circle
                  className="opacity-25"
                  cx="12"
                  cy="12"
                  r="10"
                  stroke="currentColor"
                  strokeWidth="4"
                />
                <path
                  className="opacity-75"
                  fill="currentColor"
                  d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                />
              </svg>
            )}
            {isIdle && (
              <svg className="h-5 w-5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 14.5v-9l6 4.5-6 4.5z" />
              </svg>
            )}
          </div>

          {/* Status text */}
          <span className="text-sm font-medium text-gray-700">
            {getStateLabel(playbackState)}
          </span>
        </div>

        {/* Buffer indicator */}
        <div className="flex items-center gap-2">
          <div
            className={`
              h-2 w-2 rounded-full
              ${getBufferColor(bufferStatus)}
            `}
            title={`Buffer: ${bufferStatus} (${bufferedAhead.toFixed(1)}s ahead)`}
          />
          <span className="text-xs text-gray-500">
            {bufferedAhead.toFixed(1)}s buffered
          </span>
        </div>
      </div>

      {/* Progress bar */}
      <div className="mb-4">
        <div className="relative h-2 bg-gray-200 rounded-full overflow-hidden">
          <div
            className="absolute top-0 left-0 h-full bg-primary-500 transition-all duration-300"
            style={{ width: `${Math.min(100, Math.max(0, progressPercentage))}%` }}
          />
        </div>
        <div className="flex justify-between mt-1 text-xs text-gray-500">
          <span>{formatTime(currentTime)}</span>
          <span>{duration !== null ? formatTime(duration) : '--:--'}</span>
        </div>
      </div>

      {/* Controls */}
      <div className="flex items-center justify-center gap-2">
        {/* Play/Pause button */}
        {canPlay && (
          <button
            onClick={onPlay}
            className={`
              p-3 rounded-full
              bg-primary-600 text-white
              hover:bg-primary-700 active:bg-primary-800
              transition-colors
              focus:outline-none focus:ring-2 focus:ring-primary-500 focus:ring-offset-2
            `}
            aria-label="Play"
          >
            <svg className="h-6 w-6" fill="currentColor" viewBox="0 0 24 24">
              <path d="M8 5v14l11-7z" />
            </svg>
          </button>
        )}

        {canPause && (
          <button
            onClick={onPause}
            className={`
              p-3 rounded-full
              bg-primary-600 text-white
              hover:bg-primary-700 active:bg-primary-800
              transition-colors
              focus:outline-none focus:ring-2 focus:ring-primary-500 focus:ring-offset-2
            `}
            aria-label="Pause"
          >
            <svg className="h-6 w-6" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z" />
            </svg>
          </button>
        )}

        {/* Stop button */}
        {canStop && (
          <button
            onClick={onStop}
            className={`
              p-3 rounded-full
              bg-gray-300 text-gray-700
              hover:bg-gray-400 active:bg-gray-500
              transition-colors
              focus:outline-none focus:ring-2 focus:ring-gray-500 focus:ring-offset-2
            `}
            aria-label="Stop"
          >
            <svg className="h-6 w-6" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 6h12v12H6z" />
            </svg>
          </button>
        )}
      </div>

      {/* Buffer warning */}
      {(bufferStatus === 'critical' || bufferStatus === 'empty') && isPlaying && (
        <div className="mt-3 text-center">
          <span className="text-xs text-orange-600 font-medium">
            Low buffer - playback may stutter
          </span>
        </div>
      )}
    </div>
  );
}
