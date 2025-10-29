'use client';

/**
 * TextInput Component
 *
 * Provides a textarea for user input with character counter and Speak button.
 * Supports up to 10,000 characters with real-time validation feedback.
 */

import { useState, useCallback, ChangeEvent } from 'react';
import { TTS_LIMITS, validateTTSText } from '@/types/tts';

export interface TextInputProps {
  /** Current text value */
  value: string;
  /** Callback when text changes */
  onChange: (text: string) => void;
  /** Callback when Speak button is clicked */
  onSpeak: () => void;
  /** Whether synthesis is in progress */
  isSynthesizing?: boolean;
  /** Whether the component is disabled */
  disabled?: boolean;
  /** Custom placeholder text */
  placeholder?: string;
  /** Custom class name for styling */
  className?: string;
}

export function TextInput({
  value,
  onChange,
  onSpeak,
  isSynthesizing = false,
  disabled = false,
  placeholder = 'Enter text to synthesize (up to 10,000 characters)...',
  className = '',
}: TextInputProps) {
  const [validationErrors, setValidationErrors] = useState<string[]>([]);

  // Character count
  const charCount = value.length;
  const charLimit = TTS_LIMITS.MAX_TEXT_LENGTH;
  const charPercentage = (charCount / charLimit) * 100;

  // Validate text on change
  const handleTextChange = useCallback(
    (e: ChangeEvent<HTMLTextAreaElement>) => {
      const newText = e.target.value;

      // Enforce character limit
      if (newText.length > charLimit) {
        return; // Don't allow exceeding limit
      }

      onChange(newText);

      // Update validation errors
      const errors = validateTTSText(newText);
      setValidationErrors(errors);
    },
    [onChange, charLimit]
  );

  // Handle Speak button click
  const handleSpeak = useCallback(() => {
    const errors = validateTTSText(value);

    if (errors.length > 0) {
      setValidationErrors(errors);
      return;
    }

    setValidationErrors([]);
    onSpeak();
  }, [value, onSpeak]);

  // Handle Enter key (Ctrl+Enter or Cmd+Enter to speak)
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        handleSpeak();
      }
    },
    [handleSpeak]
  );

  // Determine character counter color
  const getCounterColor = () => {
    if (charPercentage >= 100) return 'text-red-600';
    if (charPercentage >= 90) return 'text-orange-500';
    if (charPercentage >= 75) return 'text-yellow-600';
    return 'text-gray-500';
  };

  const isDisabled = disabled || isSynthesizing;
  const hasErrors = validationErrors.length > 0;

  return (
    <div className={`flex flex-col gap-3 ${className}`}>
      {/* Textarea */}
      <div className="relative">
        <textarea
          value={value}
          onChange={handleTextChange}
          onKeyDown={handleKeyDown}
          disabled={isDisabled}
          placeholder={placeholder}
          rows={8}
          className={`
            w-full px-4 py-3
            border rounded-lg
            font-sans text-base
            resize-y
            transition-colors
            focus:outline-none focus:ring-2
            disabled:opacity-50 disabled:cursor-not-allowed
            ${
              hasErrors
                ? 'border-red-300 focus:border-red-500 focus:ring-red-200'
                : 'border-gray-300 focus:border-primary-500 focus:ring-primary-200'
            }
          `}
          aria-label="Text to synthesize"
          aria-invalid={hasErrors}
          aria-describedby={hasErrors ? 'text-input-error' : 'text-input-help'}
        />

        {/* Character counter (bottom right of textarea) */}
        <div
          className={`
            absolute bottom-3 right-3
            text-sm font-medium
            ${getCounterColor()}
          `}
          aria-live="polite"
        >
          {charCount.toLocaleString()} / {charLimit.toLocaleString()}
        </div>
      </div>

      {/* Validation errors */}
      {hasErrors && (
        <div
          id="text-input-error"
          role="alert"
          className="text-sm text-red-600 font-medium"
        >
          {validationErrors.map((error, index) => (
            <div key={index}>{error}</div>
          ))}
        </div>
      )}

      {/* Help text */}
      {!hasErrors && (
        <div id="text-input-help" className="text-sm text-gray-500">
          Press <kbd className="px-2 py-1 bg-gray-100 border border-gray-300 rounded text-xs">
            Ctrl+Enter
          </kbd> to speak
        </div>
      )}

      {/* Action buttons */}
      <div className="flex items-center gap-3">
        <button
          onClick={handleSpeak}
          disabled={isDisabled || hasErrors || charCount === 0}
          className={`
            flex-1 px-6 py-3
            bg-primary-600 text-white
            font-semibold rounded-lg
            transition-all duration-200
            focus:outline-none focus:ring-2 focus:ring-primary-500 focus:ring-offset-2
            disabled:opacity-50 disabled:cursor-not-allowed
            hover:bg-primary-700 active:bg-primary-800
            ${isSynthesizing ? 'animate-pulse' : ''}
          `}
          aria-label={isSynthesizing ? 'Synthesizing...' : 'Speak text'}
        >
          {isSynthesizing ? (
            <span className="flex items-center justify-center gap-2">
              <svg
                className="animate-spin h-5 w-5"
                xmlns="http://www.w3.org/2000/svg"
                fill="none"
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
              Synthesizing...
            </span>
          ) : (
            <span className="flex items-center justify-center gap-2">
              <svg
                className="h-5 w-5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                xmlns="http://www.w3.org/2000/svg"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z"
                />
              </svg>
              Speak
            </span>
          )}
        </button>

        {/* Clear button */}
        {charCount > 0 && !isSynthesizing && (
          <button
            onClick={() => onChange('')}
            disabled={isDisabled}
            className={`
              px-4 py-3
              text-gray-700 border border-gray-300
              font-medium rounded-lg
              transition-colors
              hover:bg-gray-50 active:bg-gray-100
              focus:outline-none focus:ring-2 focus:ring-gray-500 focus:ring-offset-2
              disabled:opacity-50 disabled:cursor-not-allowed
            `}
            aria-label="Clear text"
          >
            Clear
          </button>
        )}
      </div>
    </div>
  );
}
