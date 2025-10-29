'use client';

/**
 * Main TTS Page
 *
 * Integrates TextInput, AudioPlayer, and ErrorDisplay components
 * to provide a complete text-to-speech interface.
 */

import { useState } from 'react';
import { TextInput } from '@/components/TextInput';
import { AudioPlayer } from '@/components/AudioPlayer';
import { ErrorDisplay } from '@/components/ErrorDisplay';
import { ProgressBar } from '@/components/ProgressBar';
import { useTTS } from '@/hooks/useTTS';
import { useAudioPlayer } from '@/hooks/useAudioPlayer';
import { useStreamBuffer } from '@/hooks/useStreamBuffer';

export default function TTSPage() {
  // State
  const [text, setText] = useState<string>('');
  const [chunksReceived, setChunksReceived] = useState<number>(0);
  const [estimatedTotalChunks, setEstimatedTotalChunks] = useState<number | null>(null);

  // TTS hook
  const {
    status,
    error,
    isSynthesizing,
    synthesize,
    cancel,
    clearError,
    retry,
  } = useTTS({
    autoPlay: true,
    onStart: (requestId) => {
      console.log('[TTSPage] Synthesis started:', requestId);
      setChunksReceived(0);
      // Estimate total chunks based on text length (rough: ~50 chars per chunk)
      const estimated = Math.ceil(text.length / 50);
      setEstimatedTotalChunks(estimated > 0 ? estimated : null);
    },
    onComplete: (requestId) => {
      console.log('[TTSPage] Synthesis completed:', requestId);
    },
    onError: (err) => {
      console.error('[TTSPage] Synthesis error:', err);
    },
  });

  // Audio player hook
  const {
    playbackState,
    currentTime,
    duration,
    bufferStatus,
    bufferedAhead,
    pause,
    stop,
  } = useAudioPlayer();

  // Handlers
  const handleSpeak = async () => {
    await synthesize(text);
  };

  const handleClearText = () => {
    setText('');
  };

  const handlePause = async () => {
    await pause();
  };

  const handleStop = () => {
    stop();
    cancel(); // Also cancel the TTS stream
  };

  const handleDismissError = () => {
    clearError();
  };

  const handleRetry = async () => {
    await retry();
  };

  return (
    <main className="min-h-screen bg-gradient-to-br from-blue-50 via-white to-purple-50">
      <div className="container mx-auto px-4 py-8 max-w-4xl">
        {/* Header */}
        <header className="text-center mb-8">
          <h1 className="text-4xl font-bold text-gray-900 mb-2">
            Text-to-Speech
          </h1>
          <p className="text-gray-600">
            Convert your text to natural-sounding speech using Kokoro TTS
          </p>
        </header>

        {/* Main Content */}
        <div className="space-y-6">
          {/* Error Display */}
          {error && (
            <ErrorDisplay
              error={error}
              onDismiss={handleDismissError}
              onRetry={handleRetry}
              canRetry={status === 'failed'}
            />
          )}

          {/* Text Input */}
          <section className="bg-white rounded-xl shadow-md p-6">
            <h2 className="text-xl font-semibold text-gray-800 mb-4">
              Enter Text
            </h2>
            <TextInput
              value={text}
              onChange={setText}
              onSpeak={handleSpeak}
              isSynthesizing={isSynthesizing}
            />
          </section>

          {/* Progress Bar - Show during synthesis */}
          {isSynthesizing && (
            <section className="bg-white rounded-xl shadow-md p-6">
              <ProgressBar
                chunksReceived={chunksReceived}
                totalChunks={estimatedTotalChunks}
                isComplete={status === 'completed'}
              />
            </section>
          )}

          {/* Audio Player */}
          {(status !== 'idle' || playbackState !== 'idle') && (
            <section className="bg-white rounded-xl shadow-md p-6">
              <h2 className="text-xl font-semibold text-gray-800 mb-4">
                Audio Playback
              </h2>
              <AudioPlayer
                playbackState={playbackState}
                currentTime={currentTime}
                duration={duration}
                bufferStatus={bufferStatus}
                bufferedAhead={bufferedAhead}
                onPause={handlePause}
                onStop={handleStop}
              />
            </section>
          )}

          {/* Status Info */}
          <section className="bg-white rounded-xl shadow-md p-6">
            <h2 className="text-xl font-semibold text-gray-800 mb-4">
              Status
            </h2>
            <div className="grid grid-cols-2 gap-4 text-sm">
              <div>
                <span className="text-gray-600">TTS Status:</span>
                <span className="ml-2 font-medium text-gray-900 capitalize">
                  {status}
                </span>
              </div>
              <div>
                <span className="text-gray-600">Playback:</span>
                <span className="ml-2 font-medium text-gray-900 capitalize">
                  {playbackState}
                </span>
              </div>
              <div>
                <span className="text-gray-600">Buffer:</span>
                <span className="ml-2 font-medium text-gray-900 capitalize">
                  {bufferStatus}
                </span>
              </div>
              <div>
                <span className="text-gray-600">Duration:</span>
                <span className="ml-2 font-medium text-gray-900">
                  {duration !== null ? `${duration.toFixed(1)}s` : 'Unknown'}
                </span>
              </div>
            </div>
          </section>

          {/* Help */}
          <section className="bg-blue-50 rounded-xl p-6 border border-blue-200">
            <h3 className="text-lg font-semibold text-blue-900 mb-3">
              How to Use
            </h3>
            <ol className="list-decimal list-inside space-y-2 text-blue-800">
              <li>Type or paste your text in the input field above</li>
              <li>
                Click the <strong>Speak</strong> button or press{' '}
                <kbd className="px-2 py-1 bg-white border border-blue-300 rounded text-xs">
                  Ctrl+Enter
                </kbd>
              </li>
              <li>
                Audio will start playing within 2 seconds as synthesis begins
              </li>
              <li>Use the playback controls to pause or stop the audio</li>
            </ol>
          </section>

          {/* Requirements Notice */}
          <section className="bg-yellow-50 rounded-xl p-6 border border-yellow-200">
            <h3 className="text-lg font-semibold text-yellow-900 mb-3 flex items-center gap-2">
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
                  d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
              Prerequisites
            </h3>
            <p className="text-yellow-800 text-sm">
              This application requires the RemoteMedia gRPC service to be
              running on <code className="px-1.5 py-0.5 bg-yellow-100 rounded text-xs">localhost:50051</code>.
              Please ensure the service is started before using the TTS
              functionality.
            </p>
          </section>
        </div>

        {/* Footer */}
        <footer className="mt-12 text-center text-sm text-gray-500">
          <p>
            Powered by{' '}
            <a
              href="https://github.com/remoteMedia"
              target="_blank"
              rel="noopener noreferrer"
              className="text-primary-600 hover:underline"
            >
              RemoteMedia SDK
            </a>{' '}
            and{' '}
            <a
              href="https://huggingface.co/hexgrad/Kokoro-82M"
              target="_blank"
              rel="noopener noreferrer"
              className="text-primary-600 hover:underline"
            >
              Kokoro TTS
            </a>
          </p>
        </footer>
      </div>
    </main>
  );
}
