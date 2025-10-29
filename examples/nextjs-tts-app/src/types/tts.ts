/**
 * TTS Type Definitions
 * Based on data-model.md specification
 */

/**
 * Status of a TTS request
 */
export enum TTSStatus {
  /** Request is queued and waiting to start */
  Pending = 'pending',
  /** Request is being processed and streaming */
  Streaming = 'streaming',
  /** Request completed successfully */
  Completed = 'completed',
  /** Request failed with error */
  Failed = 'failed',
  /** Request was cancelled by user */
  Cancelled = 'cancelled',
}

/**
 * Voice configuration for TTS synthesis
 */
export interface VoiceConfig {
  /** BCP-47 language code (e.g., 'en-US', 'en-GB', 'ja-JP') */
  language: string;
  /** Voice identifier (e.g., 'af', 'af_bella', 'af_sarah') */
  voice: string;
  /** Speech rate multiplier (0.5 = half speed, 2.0 = double speed) */
  speed: number;
}

/**
 * TTS error information
 */
export interface TTSError {
  /** Error code (e.g., 'GRPC_CONNECTION_FAILED', 'TTS_ENGINE_ERROR') */
  code: string;
  /** Human-readable error message */
  message: string;
  /** Timestamp when error occurred */
  timestamp: Date;
  /** Optional additional error details */
  details?: Record<string, unknown>;
}

/**
 * TTS request with metadata and status
 */
export interface TTSRequest {
  /** Unique request identifier (UUID v4) */
  id: string;
  /** Text to synthesize (max 10,000 characters) */
  text: string;
  /** Voice configuration */
  voiceConfig: VoiceConfig;
  /** Current request status */
  status: TTSStatus;
  /** Error information if status is Failed */
  error?: TTSError;
  /** Timestamp when request was created */
  createdAt: Date;
  /** Timestamp when request was last updated */
  updatedAt: Date;
}

/**
 * Default voice configuration
 */
export const DEFAULT_VOICE_CONFIG: VoiceConfig = {
  language: 'en-US',
  voice: 'af_bella',
  speed: 1.0,
};

/**
 * Voice options available in Kokoro TTS
 */
export interface VoiceOption {
  /** Voice identifier */
  id: string;
  /** Display name */
  name: string;
  /** Language code */
  language: string;
  /** Language display name */
  languageDisplay: string;
  /** Gender (if applicable) */
  gender?: 'male' | 'female' | 'neutral';
}

/**
 * Available voices (based on Kokoro TTS documentation)
 */
export const AVAILABLE_VOICES: VoiceOption[] = [
  // English (US)
  { id: 'af_bella', name: 'Bella', language: 'en-US', languageDisplay: 'English (US)', gender: 'female' },
  { id: 'af_sarah', name: 'Sarah', language: 'en-US', languageDisplay: 'English (US)', gender: 'female' },
  { id: 'af', name: 'Default', language: 'en-US', languageDisplay: 'English (US)' },
  // English (UK)
  { id: 'bf_emma', name: 'Emma', language: 'en-GB', languageDisplay: 'English (UK)', gender: 'female' },
  { id: 'bf_isabella', name: 'Isabella', language: 'en-GB', languageDisplay: 'English (UK)', gender: 'female' },
  // Add more as needed based on Kokoro TTS capabilities
];

/**
 * TTS configuration limits
 */
export const TTS_LIMITS = {
  /** Maximum text length in characters */
  MAX_TEXT_LENGTH: 10000,
  /** Minimum speech rate */
  MIN_SPEED: 0.5,
  /** Maximum speech rate */
  MAX_SPEED: 2.0,
  /** Default speech rate */
  DEFAULT_SPEED: 1.0,
} as const;

/**
 * Type guard to check if a status is terminal (no further updates expected)
 */
export function isTerminalStatus(status: TTSStatus): boolean {
  return [TTSStatus.Completed, TTSStatus.Failed, TTSStatus.Cancelled].includes(status);
}

/**
 * Type guard to check if a request can be cancelled
 */
export function isCancellable(status: TTSStatus): boolean {
  return [TTSStatus.Pending, TTSStatus.Streaming].includes(status);
}

/**
 * Validate voice configuration
 */
export function validateVoiceConfig(config: Partial<VoiceConfig>): string[] {
  const errors: string[] = [];

  if (config.speed !== undefined) {
    if (config.speed < TTS_LIMITS.MIN_SPEED || config.speed > TTS_LIMITS.MAX_SPEED) {
      errors.push(`Speed must be between ${TTS_LIMITS.MIN_SPEED} and ${TTS_LIMITS.MAX_SPEED}`);
    }
  }

  if (config.voice !== undefined && config.voice.trim() === '') {
    errors.push('Voice cannot be empty');
  }

  if (config.language !== undefined && config.language.trim() === '') {
    errors.push('Language cannot be empty');
  }

  return errors;
}

/**
 * Validate TTS text input
 */
export function validateTTSText(text: string): string[] {
  const errors: string[] = [];

  if (text.trim() === '') {
    errors.push('Text cannot be empty');
  }

  if (text.length > TTS_LIMITS.MAX_TEXT_LENGTH) {
    errors.push(`Text exceeds maximum length of ${TTS_LIMITS.MAX_TEXT_LENGTH} characters`);
  }

  return errors;
}
