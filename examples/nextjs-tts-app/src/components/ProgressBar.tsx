'use client';

/**
 * ProgressBar Component
 *
 * Displays synthesis progress for long-form text streaming.
 * Shows chunks received vs estimated total with visual progress indicator.
 */

export interface ProgressBarProps {
  /** Number of chunks received so far */
  chunksReceived: number;
  /** Estimated total chunks (null if unknown) */
  totalChunks: number | null;
  /** Whether synthesis is complete */
  isComplete: boolean;
  /** Custom class name for styling */
  className?: string;
}

/**
 * Calculate progress percentage
 */
function calculateProgress(received: number, total: number | null): number {
  if (total === null || total === 0) {
    return 0;
  }
  return Math.min(100, (received / total) * 100);
}

/**
 * Format chunk count display
 */
function formatChunkCount(received: number, total: number | null): string {
  if (total === null) {
    return `${received} chunks`;
  }
  return `${received} / ${total} chunks`;
}

export function ProgressBar({
  chunksReceived,
  totalChunks,
  isComplete,
  className = '',
}: ProgressBarProps) {
  const progress = calculateProgress(chunksReceived, totalChunks);
  const chunkDisplay = formatChunkCount(chunksReceived, totalChunks);

  // Determine status color
  const getStatusColor = () => {
    if (isComplete) return 'bg-green-500';
    if (progress > 0) return 'bg-primary-500';
    return 'bg-gray-300';
  };

  const statusColor = getStatusColor();

  // Show progress bar only if synthesis is active or complete
  if (chunksReceived === 0 && !isComplete) {
    return null;
  }

  return (
    <div className={`space-y-2 ${className}`} role="progressbar" aria-valuenow={progress} aria-valuemin={0} aria-valuemax={100}>
      {/* Header with chunk count */}
      <div className="flex items-center justify-between text-sm">
        <span className="text-gray-700 font-medium">
          Synthesis Progress
        </span>
        <span className="text-gray-600">
          {chunkDisplay}
        </span>
      </div>

      {/* Progress bar */}
      <div className="relative h-3 bg-gray-200 rounded-full overflow-hidden">
        <div
          className={`absolute top-0 left-0 h-full ${statusColor} transition-all duration-300 ease-out`}
          style={{
            width: totalChunks !== null ? `${progress}%` : '100%',
          }}
        >
          {/* Animated shimmer effect while streaming */}
          {!isComplete && (
            <div className="absolute inset-0 bg-gradient-to-r from-transparent via-white to-transparent opacity-30 animate-shimmer" />
          )}
        </div>

        {/* Indeterminate animation when total is unknown */}
        {totalChunks === null && !isComplete && (
          <div className="absolute inset-0">
            <div className="h-full w-1/3 bg-primary-600 animate-indeterminate" />
          </div>
        )}
      </div>

      {/* Status text */}
      <div className="text-xs text-gray-500 text-center">
        {isComplete ? (
          <span className="text-green-600 font-medium">
            ✓ Synthesis complete
          </span>
        ) : totalChunks !== null ? (
          <span>{progress.toFixed(0)}% complete</span>
        ) : (
          <span>Streaming in progress...</span>
        )}
      </div>

      {/* Estimated time remaining (if we have total) */}
      {!isComplete && totalChunks !== null && chunksReceived > 0 && (
        <div className="text-xs text-gray-500 text-center">
          {(() => {
            const remaining = totalChunks - chunksReceived;
            if (remaining <= 0) return null;

            // Estimate ~0.2 seconds per chunk (rough average)
            const estimatedSeconds = remaining * 0.2;
            if (estimatedSeconds < 1) {
              return 'Almost done...';
            } else if (estimatedSeconds < 10) {
              return `~${Math.ceil(estimatedSeconds)}s remaining`;
            } else {
              return `~${Math.ceil(estimatedSeconds / 10) * 10}s remaining`;
            }
          })()}
        </div>
      )}
    </div>
  );
}

// Add custom animations to globals.css:
// @keyframes shimmer {
//   0% { transform: translateX(-100%); }
//   100% { transform: translateX(100%); }
// }
//
// @keyframes indeterminate {
//   0% { transform: translateX(-100%); }
//   100% { transform: translateX(300%); }
// }
//
// .animate-shimmer {
//   animation: shimmer 2s infinite;
// }
//
// .animate-indeterminate {
//   animation: indeterminate 1.5s ease-in-out infinite;
// }
