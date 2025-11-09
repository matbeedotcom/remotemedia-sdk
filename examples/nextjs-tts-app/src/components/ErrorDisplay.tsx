'use client';

/**
 * ErrorDisplay Component
 *
 * Displays validation and synthesis errors with user-friendly messages.
 * Provides dismiss and retry actions where appropriate.
 */

import { TTSError } from '@/types/tts';

export interface ErrorDisplayProps {
  /** Error object to display */
  error: TTSError | null;
  /** Callback when user dismisses the error */
  onDismiss?: () => void;
  /** Callback when user clicks retry */
  onRetry?: () => void;
  /** Whether retry is available */
  canRetry?: boolean;
  /** Custom class name for styling */
  className?: string;
}

/**
 * Get error icon based on error code
 */
function getErrorIcon(errorCode: string): React.ReactElement {
  // Network/connection errors
  if (
    errorCode.includes('CONNECTION') ||
    errorCode.includes('NETWORK') ||
    errorCode.includes('UNAVAILABLE')
  ) {
    return (
      <svg
        className="h-5 w-5"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M18.364 5.636a9 9 0 010 12.728m0 0l-2.829-2.829m2.829 2.829L21 21M15.536 8.464a5 5 0 010 7.072m0 0l-2.829-2.829m-4.243 2.829a4.978 4.978 0 01-1.414-2.83m-1.414 5.658a9 9 0 01-2.167-9.238m7.824 2.167a1 1 0 111.414 1.414m-1.414-1.414L3 3m8.293 8.293l1.414 1.414"
        />
      </svg>
    );
  }

  // Validation errors
  if (errorCode.includes('VALIDATION')) {
    return (
      <svg
        className="h-5 w-5"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
        />
      </svg>
    );
  }

  // TTS engine errors
  if (errorCode.includes('TTS') || errorCode.includes('ENGINE')) {
    return (
      <svg
        className="h-5 w-5"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z"
        />
      </svg>
    );
  }

  // Generic error
  return (
    <svg
      className="h-5 w-5"
      fill="none"
      stroke="currentColor"
      viewBox="0 0 24 24"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
      />
    </svg>
  );
}

/**
 * Get user-friendly error title based on error code
 */
function getErrorTitle(errorCode: string): string {
  if (errorCode.includes('CONNECTION')) return 'Connection Error';
  if (errorCode.includes('VALIDATION')) return 'Invalid Input';
  if (errorCode.includes('TTS') || errorCode.includes('ENGINE')) return 'Synthesis Error';
  if (errorCode.includes('NETWORK')) return 'Network Error';
  if (errorCode.includes('UNAVAILABLE')) return 'Service Unavailable';
  return 'Error';
}

/**
 * Get user-friendly error suggestion based on error code
 */
function getErrorSuggestion(errorCode: string): string | null {
  if (errorCode.includes('CONNECTION')) {
    return 'Please check your connection and ensure the TTS service is running on localhost:50051';
  }
  if (errorCode.includes('VALIDATION')) {
    return 'Please check your input and try again';
  }
  if (errorCode.includes('UNAVAILABLE')) {
    return 'The service may be temporarily down. Please try again in a moment';
  }
  if (errorCode.includes('NETWORK')) {
    return 'Please check your internet connection and try again';
  }
  return null;
}

export function ErrorDisplay({
  error,
  onDismiss,
  onRetry,
  canRetry = false,
  className = '',
}: ErrorDisplayProps) {
  if (!error) {
    return null;
  }

  const errorIcon = getErrorIcon(error.code);
  const errorTitle = getErrorTitle(error.code);
  const errorSuggestion = getErrorSuggestion(error.code);

  return (
    <div
      className={`
        p-4 rounded-lg
        bg-red-50 border border-red-200
        ${className}
      `}
      role="alert"
      aria-live="assertive"
    >
      <div className="flex items-start gap-3">
        {/* Icon */}
        <div className="flex-shrink-0 text-red-600 mt-0.5">
          {errorIcon}
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          {/* Title */}
          <h3 className="text-sm font-semibold text-red-800 mb-1">
            {errorTitle}
          </h3>

          {/* Message */}
          <p className="text-sm text-red-700 mb-2">
            {error.message}
          </p>

          {/* Suggestion */}
          {errorSuggestion && (
            <p className="text-sm text-red-600 mb-3">
              {errorSuggestion}
            </p>
          )}

          {/* Additional details (debug info) */}
          {error.details && Object.keys(error.details).length > 0 && (
            <details className="text-xs text-red-600 mt-2">
              <summary className="cursor-pointer hover:text-red-700 font-medium">
                Technical Details
              </summary>
              <pre className="mt-2 p-2 bg-red-100 rounded overflow-x-auto">
                {JSON.stringify(error.details, null, 2)}
              </pre>
            </details>
          )}

          {/* Actions */}
          {(canRetry || onDismiss) && (
            <div className="flex items-center gap-2 mt-3">
              {canRetry && onRetry && (
                <button
                  onClick={onRetry}
                  className={`
                    px-3 py-1.5
                    text-sm font-medium
                    text-red-700 bg-red-100
                    border border-red-300
                    rounded
                    transition-colors
                    hover:bg-red-200 active:bg-red-300
                    focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2
                  `}
                  aria-label="Retry operation"
                >
                  <span className="flex items-center gap-1.5">
                    <svg
                      className="h-4 w-4"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                      />
                    </svg>
                    Retry
                  </span>
                </button>
              )}

              {onDismiss && (
                <button
                  onClick={onDismiss}
                  className={`
                    px-3 py-1.5
                    text-sm font-medium
                    text-red-600
                    hover:text-red-700
                    transition-colors
                    focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2
                    rounded
                  `}
                  aria-label="Dismiss error"
                >
                  Dismiss
                </button>
              )}
            </div>
          )}
        </div>

        {/* Close button (X) */}
        {onDismiss && (
          <button
            onClick={onDismiss}
            className={`
              flex-shrink-0
              text-red-400 hover:text-red-600
              transition-colors
              focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2
              rounded
              p-1
            `}
            aria-label="Close error"
          >
            <svg
              className="h-5 w-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}
