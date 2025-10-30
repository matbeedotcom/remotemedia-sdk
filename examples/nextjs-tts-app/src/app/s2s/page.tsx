'use client';

/**
 * Speech-to-Speech Demo Page
 *
 * Demonstrates conversational AI using LFM2-Audio-1.5B model.
 * Features:
 * - Record audio questions
 * - Get text + audio responses
 * - Multi-turn conversations with history
 * - Real-time metrics
 */

import { useState, useRef, useEffect } from 'react';
import { streamS2S, S2SAudioRecorder, deleteSession } from '@/lib/s2s-streaming-client';

interface Message {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  audio?: Float32Array;
  timestamp: Date;
}

export default function S2SDemo() {
  // State
  const [messages, setMessages] = useState<Message[]>([]);
  const [isRecording, setIsRecording] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [sessionId, setSessionId] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const [metrics, setMetrics] = useState<any>(null);

  // Refs
  const recorderRef = useRef<S2SAudioRecorder | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);

  // Initialize
  useEffect(() => {
    // Generate session ID
    const newSessionId = `s2s_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    setSessionId(newSessionId);
    console.log(`[S2S Demo] Session ID: ${newSessionId}`);

    // Initialize recorder
    recorderRef.current = new S2SAudioRecorder();

    return () => {
      // Cleanup
      if (audioContextRef.current) {
        audioContextRef.current.close();
      }
    };
  }, []);

  /**
   * Start recording user audio
   */
  const handleStartRecording = async () => {
    try {
      setError(null);
      await recorderRef.current?.startRecording();
      setIsRecording(true);
      console.log('[S2S Demo] Started recording');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to start recording';
      setError(errorMsg);
      console.error('[S2S Demo] Recording error:', err);
    }
  };

  /**
   * Stop recording and send to API
   */
  const handleStopRecording = async () => {
    try {
      setIsRecording(false);
      setIsProcessing(true);
      setError(null);

      // Stop recording and get audio
      const audioData = await recorderRef.current?.stopRecording();

      if (!audioData || audioData.length === 0) {
        throw new Error('No audio recorded');
      }

      console.log(`[S2S Demo] Recorded ${audioData.length} samples`);

      // Add user message placeholder
      const userMessageId = `user_${Date.now()}`;
      setMessages((prev) => [
        ...prev,
        {
          id: userMessageId,
          role: 'user',
          text: 'Processing audio...',
          audio: audioData,
          timestamp: new Date(),
        },
      ]);

      // Stream to API
      let assistantText = '';
      const assistantAudioChunks: Float32Array[] = [];
      const assistantMessageId = `assistant_${Date.now()}`;

      await streamS2S(
        audioData,
        {
          sessionId,
          sampleRate: 24000,
        },
        {
          onStart: () => {
            console.log('[S2S Demo] Stream started');
          },
          onText: (text) => {
            console.log(`[S2S Demo] Received text: ${text}`);
            assistantText += text;

            // Update assistant message with text
            setMessages((prev) => {
              const lastMsg = prev[prev.length - 1];
              if (lastMsg && lastMsg.id === assistantMessageId) {
                return [
                  ...prev.slice(0, -1),
                  { ...lastMsg, text: assistantText },
                ];
              } else {
                return [
                  ...prev,
                  {
                    id: assistantMessageId,
                    role: 'assistant',
                    text: assistantText,
                    timestamp: new Date(),
                  },
                ];
              }
            });
          },
          onAudio: (audioChunk, sampleRate) => {
            console.log(`[S2S Demo] Received audio: ${audioChunk.length} samples @ ${sampleRate}Hz`);
            assistantAudioChunks.push(audioChunk);

            // Play audio immediately
            playAudio(audioChunk, sampleRate);
          },
          onMetrics: (m) => {
            console.log('[S2S Demo] Metrics:', m);
            setMetrics(m);
          },
          onComplete: () => {
            console.log('[S2S Demo] Stream complete');

            // Concatenate all audio chunks
            const totalLength = assistantAudioChunks.reduce((sum, chunk) => sum + chunk.length, 0);
            const fullAudio = new Float32Array(totalLength);
            let offset = 0;
            for (const chunk of assistantAudioChunks) {
              fullAudio.set(chunk, offset);
              offset += chunk.length;
            }

            // Update or create assistant message
            setMessages((prev) => {
              const lastMsg = prev[prev.length - 1];
              if (lastMsg && lastMsg.id === assistantMessageId) {
                // Update existing assistant message with full audio
                return [
                  ...prev.slice(0, -1),
                  { ...lastMsg, audio: fullAudio },
                ];
              } else if (assistantAudioChunks.length > 0) {
                // Create assistant message if it doesn't exist (audio-only response)
                return [
                  ...prev,
                  {
                    id: assistantMessageId,
                    role: 'assistant',
                    text: '', // No text, will show "[Audio response]" in UI
                    audio: fullAudio,
                    timestamp: new Date(),
                  },
                ];
              }
              return prev;
            });

            // Update user message to show it was processed
            setMessages((prev) =>
              prev.map((msg) =>
                msg.id === userMessageId ? { ...msg, text: '[Audio question]' } : msg
              )
            );

            setIsProcessing(false);
          },
          onError: (err) => {
            console.error('[S2S Demo] Stream error:', err);
            setError(err.message);
            setIsProcessing(false);
          },
        }
      );
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to process audio';
      setError(errorMsg);
      console.error('[S2S Demo] Processing error:', err);
      setIsProcessing(false);
    }
  };

  /**
   * Play audio using Web Audio API
   */
  const playAudio = async (audioData: Float32Array, sampleRate: number) => {
    try {
      // Create audio context if needed
      if (!audioContextRef.current) {
        audioContextRef.current = new AudioContext({ sampleRate });
      }

      const audioContext = audioContextRef.current;

      // Create audio buffer
      const audioBuffer = audioContext.createBuffer(1, audioData.length, sampleRate);
      audioBuffer.getChannelData(0).set(audioData);

      // Create buffer source
      const source = audioContext.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(audioContext.destination);
      source.start();

      console.log(`[S2S Demo] Playing ${audioData.length} samples @ ${sampleRate}Hz`);
    } catch (err) {
      console.error('[S2S Demo] Audio playback error:', err);
    }
  };

  /**
   * Reset conversation
   */
  const handleResetConversation = async () => {
    try {
      if (sessionId) {
        await deleteSession(sessionId);
      }
      setMessages([]);
      setMetrics(null);
      setError(null);
      // Generate new session ID
      const newSessionId = `s2s_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      setSessionId(newSessionId);
      console.log(`[S2S Demo] Conversation reset. New session: ${newSessionId}`);
    } catch (err) {
      console.error('[S2S Demo] Reset error:', err);
    }
  };

  return (
    <main className="min-h-screen bg-gradient-to-br from-purple-50 via-white to-blue-50">
      <div className="container mx-auto px-4 py-8 max-w-6xl">
        {/* Header */}
        <header className="text-center mb-8">
          <div className="text-6xl mb-4">🗣️</div>
          <h1 className="text-5xl font-bold bg-gradient-to-r from-purple-600 to-blue-600 bg-clip-text text-transparent mb-4">
            Speech-to-Speech Conversation
          </h1>
          <p className="text-xl text-gray-600 max-w-2xl mx-auto">
            Powered by <strong>LFM2-Audio-1.5B</strong> - Natural voice conversations with AI
          </p>
        </header>

        {/* Error Display */}
        {error && (
          <div className="mb-6 p-4 bg-red-50 border border-red-200 rounded-lg">
            <div className="flex items-start gap-2">
              <span className="text-red-600 text-xl">⚠️</span>
              <div>
                <div className="font-semibold text-red-900">Error</div>
                <div className="text-red-700 text-sm">{error}</div>
              </div>
            </div>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Left Column - Controls */}
          <div className="lg:col-span-1">
            <div className="bg-white rounded-xl shadow-lg p-6 border border-gray-200 sticky top-8">
              <h2 className="text-2xl font-bold text-gray-900 mb-4">Controls</h2>

              {/* Recording Button */}
              <div className="mb-6">
                {!isRecording && !isProcessing && (
                  <button
                    onClick={handleStartRecording}
                    className="w-full py-4 rounded-lg bg-purple-600 text-white hover:bg-purple-700 active:bg-purple-800 transition-colors font-semibold text-lg flex items-center justify-center gap-2"
                  >
                    <span className="text-2xl">🎤</span>
                    Start Recording
                  </button>
                )}

                {isRecording && (
                  <button
                    onClick={handleStopRecording}
                    className="w-full py-4 rounded-lg bg-red-600 text-white hover:bg-red-700 active:bg-red-800 transition-colors font-semibold text-lg flex items-center justify-center gap-2 animate-pulse"
                  >
                    <span className="text-2xl">⏹️</span>
                    Stop Recording
                  </button>
                )}

                {isProcessing && (
                  <div className="w-full py-4 rounded-lg bg-gray-300 text-gray-700 font-semibold text-lg flex items-center justify-center gap-2">
                    <span className="animate-spin text-2xl">⏳</span>
                    Processing...
                  </div>
                )}
              </div>

              {/* Reset Button */}
              <button
                onClick={handleResetConversation}
                disabled={isRecording || isProcessing}
                className="w-full py-3 rounded-lg bg-gray-200 text-gray-700 hover:bg-gray-300 active:bg-gray-400 transition-colors font-medium disabled:opacity-50 disabled:cursor-not-allowed"
              >
                🔄 Reset Conversation
              </button>

              {/* Session Info */}
              <div className="mt-6 p-4 bg-gray-50 rounded-lg">
                <h3 className="font-semibold text-gray-900 mb-2">Session Info</h3>
                <div className="text-xs text-gray-600 break-all">
                  <strong>ID:</strong> {sessionId}
                </div>
                <div className="text-xs text-gray-600 mt-1">
                  <strong>Messages:</strong> {messages.length}
                </div>
              </div>

              {/* Metrics */}
              {metrics && (
                <div className="mt-4 p-4 bg-gradient-to-br from-purple-50 to-blue-50 rounded-lg">
                  <h3 className="font-semibold text-gray-900 mb-2">Metrics</h3>
                  <div className="space-y-1 text-xs text-gray-700">
                    {metrics.cacheHitRate !== undefined && (
                      <div>
                        <strong>Cache Hit Rate:</strong> {(metrics.cacheHitRate * 100).toFixed(1)}%
                      </div>
                    )}
                    {metrics.averageLatencyMs !== undefined && (
                      <div>
                        <strong>Avg Latency:</strong> {metrics.averageLatencyMs.toFixed(0)}ms
                      </div>
                    )}
                    {metrics.cachedNodesCount !== undefined && (
                      <div>
                        <strong>Cached Nodes:</strong> {metrics.cachedNodesCount}
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Right Column - Conversation */}
          <div className="lg:col-span-2">
            <div className="bg-white rounded-xl shadow-lg p-6 border border-gray-200 min-h-[600px]">
              <h2 className="text-2xl font-bold text-gray-900 mb-4">Conversation</h2>

              {/* Messages */}
              <div className="space-y-4">
                {messages.length === 0 && (
                  <div className="text-center text-gray-500 py-12">
                    <div className="text-4xl mb-4">💬</div>
                    <div>Click "Start Recording" to begin a conversation</div>
                  </div>
                )}

                {messages.map((message) => (
                  <div
                    key={message.id}
                    className={`p-4 rounded-lg ${
                      message.role === 'user'
                        ? 'bg-purple-50 border border-purple-200'
                        : 'bg-blue-50 border border-blue-200'
                    }`}
                  >
                    <div className="flex items-start gap-3">
                      <div className="text-2xl">
                        {message.role === 'user' ? '👤' : '🤖'}
                      </div>
                      <div className="flex-1">
                        <div className="font-semibold text-gray-900 mb-1">
                          {message.role === 'user' ? 'You' : 'AI Assistant'}
                        </div>
                        <div className="text-gray-700">
                          {message.text || (message.audio && message.role === 'assistant' ? '[Audio response]' : '')}
                        </div>
                        {message.audio && (
                          <div className="mt-2 text-xs text-gray-500">
                            🔊 Audio: {message.audio.length} samples (
                            {(message.audio.length / 24000).toFixed(2)}s)
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>

        {/* Footer */}
        <footer className="mt-12 text-center text-gray-500 text-sm">
          <p>
            Powered by <strong>Liquid AI LFM2-Audio-1.5B</strong> | RemoteMedia SDK
          </p>
        </footer>
      </div>
    </main>
  );
}
