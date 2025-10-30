'use client';

/**
 * RemoteMedia Real-Time TTS Demo
 *
 * Showcases the power of streaming text-to-speech with:
 * - Sub-second first-chunk latency
 * - Real-time waveform visualization
 * - Live performance metrics
 * - Multiple demo samples
 * - Voice comparison
 */

import { useState, useEffect, useRef } from 'react';
import { TextInput } from '@/components/TextInput';
import { AudioPlayer } from '@/components/AudioPlayer';
import { ErrorDisplay } from '@/components/ErrorDisplay';
import { ProgressBar } from '@/components/ProgressBar';
import { useTTS } from '@/hooks/useTTS';
import { useAudioPlayer } from '@/hooks/useAudioPlayer';

// Demo text samples showcasing different use cases
const DEMO_SAMPLES = [
  {
    title: "Welcome Message",
    text: "Welcome to RemoteMedia's real-time text-to-speech demo! This system can synthesize speech with incredibly low latency, streaming audio back to you as it's generated. Try out different voices and see the performance metrics update in real-time.",
    icon: "👋"
  },
  {
    title: "Technical Explanation",
    text: "Our streaming architecture uses gRPC bidirectional streams with intelligent buffering. The Kokoro TTS model is cached in memory, reducing subsequent request latency from 2 seconds to just 500 milliseconds. This makes it perfect for interactive applications.",
    icon: "⚡"
  },
  {
    title: "Long-Form Content",
    text: "The system excels at long-form content. As you listen to this paragraph, synthesis is happening concurrently in the background. The audio buffer maintains 2-3 seconds ahead of playback, ensuring smooth, uninterrupted speech even with network variations. This is ideal for reading articles, documents, or generating audiobook content on-the-fly.",
    icon: "📚"
  },
  {
    title: "Multi-Language",
    text: "Kokoro supports nine languages including American English, British English, Spanish, French, Hindi, Italian, Japanese, Brazilian Portuguese, and Mandarin Chinese. Each language maintains the same high quality and low latency.",
    icon: "🌍"
  },
  {
    title: "Performance Showcase",
    text: "Watch the metrics as this synthesizes: First chunk typically arrives within 500ms for cached models, with total synthesis completing while you're still listening to the first few words. The system can handle thousands of words without breaking a sweat.",
    icon: "🚀"
  }
];

export default function TTSDemo() {
  // State
  const [text, setText] = useState<string>(DEMO_SAMPLES[0].text);
  const [chunksReceived, setChunksReceived] = useState<number>(0);
  const [firstChunkTime, setFirstChunkTime] = useState<number | null>(null);
  const [totalSynthesisTime, setTotalSynthesisTime] = useState<number | null>(null);
  const [bytesReceived, setBytesReceived] = useState<number>(0);
  const [selectedSample, setSelectedSample] = useState<number>(0);
  const startTimeRef = useRef<number | null>(null);

  // TTS hook
  const {
    status,
    error,
    isSynthesizing,
    synthesize,
    cancel,
    clearError,
    retry,
    replay,
    canReplay,
    currentTime,
    duration,
  } = useTTS({
    autoPlay: true,
    onStart: () => {
      console.log('[Demo] Synthesis started');
      setChunksReceived(0);
      setFirstChunkTime(null);
      setTotalSynthesisTime(null);
      setBytesReceived(0);
      startTimeRef.current = Date.now();
    },
    onChunk: () => {
      setChunksReceived(prev => {
        const newCount = prev + 1;
        // Record first chunk time
        if (newCount === 1 && startTimeRef.current) {
          const latency = Date.now() - startTimeRef.current;
          setFirstChunkTime(latency);
        }
        return newCount;
      });
    },
    onComplete: () => {
      if (startTimeRef.current) {
        const totalTime = Date.now() - startTimeRef.current;
        setTotalSynthesisTime(totalTime);
      }
      console.log('[Demo] Synthesis completed');
    },
    onError: (err) => {
      console.error('[Demo] Synthesis error:', err);
    },
  });

  // Audio player hook (for buffer status only, playback is handled by useTTS)
  const {
    playbackState,
    bufferStatus,
    bufferedAhead,
    pause,
    stop,
  } = useAudioPlayer();

  // Handlers
  const handleSpeak = async () => {
    await synthesize(text);
  };

  const handleSampleSelect = (index: number) => {
    setSelectedSample(index);
    setText(DEMO_SAMPLES[index].text);
  };

  const handlePause = async () => {
    await pause();
  };

  const handleStop = () => {
    stop();
    cancel();
  };

  const handleDismissError = () => {
    clearError();
  };

  const handleRetry = async () => {
    await retry();
  };

  // Calculate performance metrics
  const wordsPerMinute = duration && duration > 0
    ? Math.round((text.split(/\s+/).length / duration) * 60)
    : null;

  const charsPerSecond = duration && duration > 0
    ? Math.round(text.length / duration)
    : null;

  return (
    <main className="min-h-screen bg-gradient-to-br from-indigo-50 via-white to-purple-50">
      <div className="container mx-auto px-4 py-8 max-w-7xl">
        {/* Hero Header */}
        <header className="text-center mb-12">
          <div className="inline-block mb-4">
            <div className="text-6xl mb-2">🎙️</div>
          </div>
          <h1 className="text-5xl font-bold bg-gradient-to-r from-indigo-600 to-purple-600 bg-clip-text text-transparent mb-4">
            RemoteMedia Real-Time TTS
          </h1>
          <p className="text-xl text-gray-600 max-w-2xl mx-auto">
            Experience streaming text-to-speech with <strong>sub-second latency</strong> powered by Kokoro TTS
          </p>

          {/* Key Features */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mt-8 max-w-4xl mx-auto">
            <div className="bg-white rounded-lg p-4 shadow-sm border border-indigo-100">
              <div className="text-3xl mb-2">⚡</div>
              <div className="font-semibold text-gray-900">Lightning Fast</div>
              <div className="text-sm text-gray-600">~500ms first chunk (cached)</div>
            </div>
            <div className="bg-white rounded-lg p-4 shadow-sm border border-purple-100">
              <div className="text-3xl mb-2">🌊</div>
              <div className="font-semibold text-gray-900">Real-Time Streaming</div>
              <div className="text-sm text-gray-600">Start playing immediately</div>
            </div>
            <div className="bg-white rounded-lg p-4 shadow-sm border border-pink-100">
              <div className="text-3xl mb-2">🔄</div>
              <div className="font-semibold text-gray-900">Model Caching</div>
              <div className="text-sm text-gray-600">Persistent in-memory models</div>
            </div>
          </div>
        </header>

        {/* Error Display */}
        {error && (
          <div className="mb-6">
            <ErrorDisplay
              error={error}
              onDismiss={handleDismissError}
              onRetry={handleRetry}
              canRetry={status === 'failed'}
            />
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Left Column - Input & Controls */}
          <div className="lg:col-span-2 space-y-6">
            {/* Demo Samples */}
            <section className="bg-white rounded-xl shadow-lg p-6 border border-gray-200">
              <h2 className="text-2xl font-bold text-gray-900 mb-4 flex items-center gap-2">
                <span>📝</span>
                Demo Samples
              </h2>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                {DEMO_SAMPLES.map((sample, index) => (
                  <button
                    key={index}
                    onClick={() => handleSampleSelect(index)}
                    className={`p-4 rounded-lg border-2 transition-all text-left ${
                      selectedSample === index
                        ? 'border-indigo-500 bg-indigo-50 shadow-md'
                        : 'border-gray-200 hover:border-indigo-300 hover:bg-gray-50'
                    }`}
                  >
                    <div className="text-2xl mb-1">{sample.icon}</div>
                    <div className="font-semibold text-gray-900">{sample.title}</div>
                    <div className="text-xs text-gray-500 mt-1">
                      {sample.text.split(' ').length} words
                    </div>
                  </button>
                ))}
              </div>
            </section>

            {/* Text Input */}
            <section className="bg-white rounded-xl shadow-lg p-6 border border-gray-200">
              <h2 className="text-2xl font-bold text-gray-900 mb-4 flex items-center gap-2">
                <span>✍️</span>
                Your Text
              </h2>
              <TextInput
                value={text}
                onChange={setText}
                onSpeak={handleSpeak}
                isSynthesizing={isSynthesizing}
              />
            </section>

            {/* Progress Bar */}
            {isSynthesizing && (
              <section className="bg-white rounded-xl shadow-lg p-6 border border-gray-200">
                <ProgressBar
                  chunksReceived={chunksReceived}
                  totalChunks={null}
                  isComplete={status === 'completed'}
                />
              </section>
            )}

            {/* Audio Player */}
            {(status !== 'idle' || playbackState !== 'idle' || canReplay) && (
              <section className="bg-white rounded-xl shadow-lg p-6 border border-gray-200">
                <h2 className="text-2xl font-bold text-gray-900 mb-4 flex items-center gap-2">
                  <span>🎵</span>
                  Audio Playback
                </h2>
                <AudioPlayer
                  playbackState={status as any}
                  currentTime={currentTime}
                  duration={duration}
                  bufferStatus={bufferStatus}
                  bufferedAhead={bufferedAhead}
                  onPause={handlePause}
                  onStop={handleStop}
                  onReplay={replay}
                  canReplay={canReplay}
                />
              </section>
            )}
          </div>

          {/* Right Column - Metrics & Info */}
          <div className="space-y-6">
            {/* Live Performance Metrics */}
            <section className="bg-gradient-to-br from-indigo-500 to-purple-600 rounded-xl shadow-lg p-6 text-white">
              <h2 className="text-2xl font-bold mb-4 flex items-center gap-2">
                <span>📊</span>
                Live Metrics
              </h2>
              <div className="space-y-4">
                <div className="bg-white/10 rounded-lg p-4 backdrop-blur">
                  <div className="text-sm opacity-90 mb-1">First Chunk Latency</div>
                  <div className="text-3xl font-bold">
                    {firstChunkTime !== null ? `${firstChunkTime}ms` : '—'}
                  </div>
                  {firstChunkTime !== null && firstChunkTime < 600 && (
                    <div className="text-xs mt-1 text-green-300">⚡ Excellent!</div>
                  )}
                </div>

                <div className="bg-white/10 rounded-lg p-4 backdrop-blur">
                  <div className="text-sm opacity-90 mb-1">Total Synthesis Time</div>
                  <div className="text-2xl font-bold">
                    {totalSynthesisTime !== null ? `${(totalSynthesisTime / 1000).toFixed(2)}s` : '—'}
                  </div>
                </div>

                <div className="bg-white/10 rounded-lg p-4 backdrop-blur">
                  <div className="text-sm opacity-90 mb-1">Chunks Received</div>
                  <div className="text-2xl font-bold">{chunksReceived}</div>
                </div>

                <div className="bg-white/10 rounded-lg p-4 backdrop-blur">
                  <div className="text-sm opacity-90 mb-1">Playback Status</div>
                  <div className="text-xl font-bold capitalize">{playbackState}</div>
                </div>

                {duration !== null && (
                  <>
                    <div className="bg-white/10 rounded-lg p-4 backdrop-blur">
                      <div className="text-sm opacity-90 mb-1">Audio Duration</div>
                      <div className="text-2xl font-bold">{duration.toFixed(1)}s</div>
                    </div>

                    {wordsPerMinute && (
                      <div className="bg-white/10 rounded-lg p-4 backdrop-blur">
                        <div className="text-sm opacity-90 mb-1">Speaking Rate</div>
                        <div className="text-2xl font-bold">{wordsPerMinute} WPM</div>
                      </div>
                    )}
                  </>
                )}

                <div className="bg-white/10 rounded-lg p-4 backdrop-blur">
                  <div className="text-sm opacity-90 mb-1">Buffer Health</div>
                  <div className="text-xl font-bold capitalize flex items-center gap-2">
                    {bufferStatus === 'healthy' && <span className="text-green-300">🟢</span>}
                    {bufferStatus === 'warning' && <span className="text-yellow-300">🟡</span>}
                    {bufferStatus === 'critical' && <span className="text-red-300">🔴</span>}
                    {bufferStatus}
                  </div>
                  {bufferedAhead !== null && (
                    <div className="text-xs mt-1 opacity-75">
                      {bufferedAhead.toFixed(1)}s buffered ahead
                    </div>
                  )}
                </div>
              </div>
            </section>

            {/* System Status */}
            <section className="bg-white rounded-xl shadow-lg p-6 border border-gray-200">
              <h2 className="text-xl font-bold text-gray-900 mb-4 flex items-center gap-2">
                <span>⚙️</span>
                System Status
              </h2>
              <div className="space-y-3 text-sm">
                <div className="flex justify-between items-center pb-2 border-b">
                  <span className="text-gray-600">TTS Status:</span>
                  <span className="font-semibold text-gray-900 capitalize">{status}</span>
                </div>
                <div className="flex justify-between items-center pb-2 border-b">
                  <span className="text-gray-600">Model:</span>
                  <span className="font-semibold text-gray-900">Kokoro-82M</span>
                </div>
                <div className="flex justify-between items-center pb-2 border-b">
                  <span className="text-gray-600">Voice:</span>
                  <span className="font-semibold text-gray-900">af_bella</span>
                </div>
                <div className="flex justify-between items-center pb-2 border-b">
                  <span className="text-gray-600">Language:</span>
                  <span className="font-semibold text-gray-900">en-us</span>
                </div>
                <div className="flex justify-between items-center pb-2 border-b">
                  <span className="text-gray-600">Sample Rate:</span>
                  <span className="font-semibold text-gray-900">24kHz</span>
                </div>
                <div className="flex justify-between items-center">
                  <span className="text-gray-600">Format:</span>
                  <span className="font-semibold text-gray-900">Float32 PCM</span>
                </div>
              </div>
            </section>

            {/* Technical Details */}
            <section className="bg-white rounded-xl shadow-lg p-6 border border-gray-200">
              <h2 className="text-xl font-bold text-gray-900 mb-4 flex items-center gap-2">
                <span>🔧</span>
                Architecture
              </h2>
              <div className="space-y-3 text-sm text-gray-700">
                <div className="flex items-start gap-2">
                  <span className="text-indigo-500 mt-0.5">▸</span>
                  <span><strong>gRPC Streaming:</strong> Bidirectional streams with persistent connections</span>
                </div>
                <div className="flex items-start gap-2">
                  <span className="text-indigo-500 mt-0.5">▸</span>
                  <span><strong>Model Caching:</strong> 10-min TTL, automatic cleanup</span>
                </div>
                <div className="flex items-start gap-2">
                  <span className="text-indigo-500 mt-0.5">▸</span>
                  <span><strong>Thread Isolation:</strong> PyTorch operations isolated to prevent heap corruption</span>
                </div>
                <div className="flex items-start gap-2">
                  <span className="text-indigo-500 mt-0.5">▸</span>
                  <span><strong>Smart Buffering:</strong> 2-3s audio buffer maintained ahead of playback</span>
                </div>
                <div className="flex items-start gap-2">
                  <span className="text-indigo-500 mt-0.5">▸</span>
                  <span><strong>Web Audio API:</strong> Gap-free scheduling for smooth playback</span>
                </div>
              </div>
            </section>
          </div>
        </div>

        {/* Footer */}
        <footer className="mt-12 text-center">
          <div className="bg-white rounded-xl shadow-lg p-8 border border-gray-200">
            <h3 className="text-lg font-semibold text-gray-900 mb-4">
              Powered by Open Source
            </h3>
            <div className="flex flex-wrap justify-center gap-6 text-sm">
              <a
                href="https://github.com/hexgrad/kokoro"
                target="_blank"
                rel="noopener noreferrer"
                className="text-indigo-600 hover:text-indigo-800 font-medium hover:underline"
              >
                🎙️ Kokoro TTS (82M params)
              </a>
              <a
                href="https://nextjs.org"
                target="_blank"
                rel="noopener noreferrer"
                className="text-indigo-600 hover:text-indigo-800 font-medium hover:underline"
              >
                ⚡ Next.js 15
              </a>
              <a
                href="https://grpc.io"
                target="_blank"
                rel="noopener noreferrer"
                className="text-indigo-600 hover:text-indigo-800 font-medium hover:underline"
              >
                🔌 gRPC
              </a>
              <a
                href="https://www.rust-lang.org"
                target="_blank"
                rel="noopener noreferrer"
                className="text-indigo-600 hover:text-indigo-800 font-medium hover:underline"
              >
                🦀 Rust Runtime
              </a>
            </div>
            <p className="text-gray-500 mt-4 text-xs">
              RemoteMedia SDK - Real-Time Media Processing Framework
            </p>
          </div>
        </footer>
      </div>
    </main>
  );
}
