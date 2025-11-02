'use client';

/**
 * VAD-based Speech-to-Speech Demo Page
 *
 * Continuous conversation with automatic speech detection.
 * Features:
 * - Always-on microphone with VAD
 * - Automatic speech start/end detection
 * - Hands-free conversation
 * - Real-time audio streaming
 */

import { useState, useRef, useEffect } from 'react';

interface Message {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  audio?: Float32Array;
  timestamp: Date;
}

export default function VADS2SDemo() {
  // State
  const [messages, setMessages] = useState<Message[]>([]);
  const [isActive, setIsActive] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [isSpeaking, setIsSpeaking] = useState(false);
  const [sessionId, setSessionId] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const [metrics, setMetrics] = useState<any>(null);

  // Refs
  const audioContextRef = useRef<AudioContext | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const sessionInitializedRef = useRef(false);
  const isActiveRef = useRef(false);
  const resultsAbortControllerRef = useRef<AbortController | null>(null);

  // Initialize session on mount
  useEffect(() => {
    const newSessionId = `vad_s2s_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    setSessionId(newSessionId);
    console.log(`[VAD S2S Demo] Session ID: ${newSessionId}`);
  }, []);

  /**
   * Initialize VAD session with backend
   */
  const initializeSession = async (sid: string) => {
    if (sessionInitializedRef.current) return;

    try {
      console.log(`[VAD S2S] Initializing session: ${sid}`);

      const response = await fetch('/api/s2s/vad-stream', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sessionId: sid,
          systemPrompt: 'You are a helpful AI assistant. Respond naturally and conversationally.',
        }),
      });

      if (!response.ok) {
        throw new Error(`Session init failed: ${response.statusText}`);
      }

      const reader = response.body?.getReader();
      const decoder = new TextDecoder();

      if (!reader) throw new Error('No response body');

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const chunk = decoder.decode(value);
        const lines = chunk.split('\n\n');

        for (const line of lines) {
          if (!line.trim() || !line.startsWith('data: ')) continue;

          const data = JSON.parse(line.substring(6));
          console.log('[VAD S2S] Session event:', data);

          if (data.type === 'ready') {
            sessionInitializedRef.current = true;
            console.log('[VAD S2S] Session ready!');

            // Start listening to results
            startResultsListener(sid);
            return;
          }
        }
      }
    } catch (err) {
      console.error('[VAD S2S] Session init error:', err);
      throw err;
    }
  };

  /**
   * Start listening to pipeline results via SSE
   */
  const startResultsListener = async (sid: string) => {
    try {
      console.log(`[VAD S2S] Starting results listener for session: ${sid}`);

      // Cancel previous listener if exists
      if (resultsAbortControllerRef.current) {
        resultsAbortControllerRef.current.abort();
      }

      const abortController = new AbortController();
      resultsAbortControllerRef.current = abortController;

      const response = await fetch(`/api/s2s/results?sessionId=${sid}`, {
        signal: abortController.signal,
      });

      if (!response.ok) {
        throw new Error(`Results listener failed: ${response.statusText}`);
      }

      const reader = response.body?.getReader();
      const decoder = new TextDecoder();

      if (!reader) throw new Error('No response body');

      // Process results stream
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const chunk = decoder.decode(value);
        const lines = chunk.split('\n\n');

        for (const line of lines) {
          if (!line.trim() || !line.startsWith('data: ')) continue;

          const data = JSON.parse(line.substring(6));
          console.log('[VAD S2S] Result event:', data);

          // Handle result events
          handleStreamResponse(data);
        }
      }
    } catch (err) {
      if (err instanceof Error && err.name === 'AbortError') {
        console.log('[VAD S2S] Results listener cancelled');
      } else {
        console.error('[VAD S2S] Results listener error:', err);
      }
    }
  };

  /**
   * Start continuous microphone streaming
   */
  const handleStart = async () => {
    try {
      setError(null);

      // Initialize session first
      await initializeSession(sessionId);

      // Get microphone access
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          sampleRate: 16000,
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
        },
      });

      mediaStreamRef.current = stream;

      // Create audio context
      const audioContext = new AudioContext({ sampleRate: 16000 });
      audioContextRef.current = audioContext;

      const source = audioContext.createMediaStreamSource(stream);
      const processor = audioContext.createScriptProcessor(4096, 1, 1);
      processorRef.current = processor;

      // Process audio chunks
      processor.onaudioprocess = (e) => {
        if (!isActiveRef.current) return;

        const inputData = e.inputBuffer.getChannelData(0);
        const audioChunk = new Float32Array(inputData);

        console.log(`[VAD S2S] Captured audio chunk: ${audioChunk.length} samples`);

        // Send chunk to backend via VAD pipeline (don't await - fire and forget)
        sendAudioChunk(audioChunk).catch((err) => {
          console.error('[VAD S2S] Failed to send chunk:', err);
        });
      };

      source.connect(processor);
      processor.connect(audioContext.destination);

      setIsActive(true);
      isActiveRef.current = true;
      console.log('[VAD S2S] Microphone streaming started');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to start microphone';
      setError(errorMsg);
      console.error('[VAD S2S] Start error:', err);
    }
  };

  /**
   * Stop continuous streaming
   */
  const handleStop = () => {
    setIsActive(false);
    isActiveRef.current = false;

    if (processorRef.current) {
      processorRef.current.disconnect();
      processorRef.current = null;
    }

    if (mediaStreamRef.current) {
      mediaStreamRef.current.getTracks().forEach((track) => track.stop());
      mediaStreamRef.current = null;
    }

    if (audioContextRef.current) {
      audioContextRef.current.close();
      audioContextRef.current = null;
    }

    // Stop results listener
    if (resultsAbortControllerRef.current) {
      resultsAbortControllerRef.current.abort();
      resultsAbortControllerRef.current = null;
    }

    console.log('[VAD S2S] Microphone streaming stopped');
  };

  /**
   * Send audio chunk to VAD pipeline
   */
  const sendAudioChunk = async (audioData: Float32Array) => {
    // console.log(`[VAD S2S] sendAudioChunk called with ${audioData.length} samples, isActive=${isActiveRef.current}`);

    if (!isActiveRef.current) return;

    try {
      // console.log('[VAD S2S] Converting audio to base64...');
      // Convert Float32Array to base64
      const buffer = new ArrayBuffer(audioData.length * 4);
      const view = new DataView(buffer);
      for (let i = 0; i < audioData.length; i++) {
        view.setFloat32(i * 4, audioData[i], true);
      }
      const base64Audio = btoa(String.fromCharCode(...new Uint8Array(buffer)));

      // console.log(`[VAD S2S] Sending to /api/s2s/vad-stream with sessionId=${sessionId}`);

      // Send to VAD pipeline
      const response = await fetch('/api/s2s/vad-stream', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          audio: base64Audio,
          sampleRate: 16000,
          sessionId: sessionId,
        }),
      });

      if (!response.ok) {
        // console.error('[VAD S2S] Chunk send failed:', response.statusText);
        return;
      }

      // Read acknowledgment response (no longer blocking on results)
      const reader = response.body?.getReader();
      const decoder = new TextDecoder();

      if (!reader) return;

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const chunk = decoder.decode(value);
        const lines = chunk.split('\n\n');

        for (const line of lines) {
          if (!line.trim() || !line.startsWith('data: ')) continue;

          const data = JSON.parse(line.substring(6));
          // console.log('[VAD S2S] Chunk acknowledgment:', data);
        }
      }
    } catch (err) {
      // console.error('[VAD S2S] Send chunk error:', err);
    }
  };

  /**
   * Handle streaming response from backend
   */
  const handleStreamResponse = (data: any) => {
    console.log('[VAD S2S] Response:', data);

    switch (data.type) {
      case 'connected':
        console.log('[VAD S2S] Results listener connected');
        break;

      case 'heartbeat':
        // Ignore heartbeat
        break;

      case 'text':
        // Add assistant text message
        setMessages((prev) => [
          ...prev,
          {
            id: `assistant_${Date.now()}`,
            role: 'assistant',
            text: data.content,
            timestamp: new Date(),
          },
        ]);
        setIsProcessing(false);
        break;

      case 'audio':
        // Decode and play audio
        const audioBuffer = Uint8Array.from(atob(data.content), (c) => c.charCodeAt(0));
        const float32Audio = new Float32Array(audioBuffer.buffer);
        playAudio(float32Audio, data.sampleRate || 24000);
        break;

      case 'json':
        console.log('[VAD S2S] JSON output:', data.content);
        break;

      case 'metrics':
        setMetrics(data.content);
        break;

      case 'complete':
        setIsProcessing(false);
        break;

      case 'error':
        setError(data.content);
        setIsProcessing(false);
        break;
    }
  };

  /**
   * Play audio using Web Audio API
   */
  const playAudio = async (audioData: Float32Array, sampleRate: number) => {
    try {
      if (!audioContextRef.current) {
        audioContextRef.current = new AudioContext({ sampleRate });
      }

      const audioContext = audioContextRef.current;
      const audioBuffer = audioContext.createBuffer(1, audioData.length, sampleRate);
      audioBuffer.getChannelData(0).set(audioData);

      const source = audioContext.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(audioContext.destination);
      source.start();

      console.log(`[VAD S2S] Playing ${audioData.length} samples @ ${sampleRate}Hz`);
    } catch (err) {
      console.error('[VAD S2S] Audio playback error:', err);
    }
  };

  /**
   * Reset conversation
   */
  const handleReset = async () => {
    if (isActive) handleStop();

    setMessages([]);
    setMetrics(null);
    setError(null);
    sessionInitializedRef.current = false;

    // Stop results listener
    if (resultsAbortControllerRef.current) {
      resultsAbortControllerRef.current.abort();
      resultsAbortControllerRef.current = null;
    }

    const newSessionId = `vad_s2s_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    setSessionId(newSessionId);
    console.log(`[VAD S2S] Reset - New session: ${newSessionId}`);
  };

  return (
    <main className="min-h-screen bg-gradient-to-br from-green-50 via-white to-blue-50">
      <div className="container mx-auto px-4 py-8 max-w-6xl">
        {/* Header */}
        <header className="text-center mb-8">
          <div className="text-6xl mb-4">🎙️</div>
          <h1 className="text-5xl font-bold bg-gradient-to-r from-green-600 to-blue-600 bg-clip-text text-transparent mb-4">
            VAD Speech-to-Speech
          </h1>
          <p className="text-xl text-gray-600 max-w-2xl mx-auto">
            Hands-free conversation with automatic speech detection
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

              {/* Status Indicator */}
              <div className="mb-6 p-4 bg-gray-50 rounded-lg">
                <div className="flex items-center gap-2 mb-2">
                  <div
                    className={`w-3 h-3 rounded-full ${
                      isActive ? 'bg-green-500 animate-pulse' : 'bg-gray-300'
                    }`}
                  />
                  <span className="font-semibold">
                    {isActive ? 'Listening' : 'Inactive'}
                  </span>
                </div>
                {isSpeaking && (
                  <div className="text-sm text-green-600">🗣️ Speech detected</div>
                )}
                {isProcessing && (
                  <div className="text-sm text-blue-600">⏳ Processing...</div>
                )}
              </div>

              {/* Start/Stop Button */}
              <div className="mb-6">
                {!isActive ? (
                  <button
                    onClick={handleStart}
                    className="w-full py-4 rounded-lg bg-green-600 text-white hover:bg-green-700 active:bg-green-800 transition-colors font-semibold text-lg flex items-center justify-center gap-2"
                  >
                    <span className="text-2xl">▶️</span>
                    Start Listening
                  </button>
                ) : (
                  <button
                    onClick={handleStop}
                    className="w-full py-4 rounded-lg bg-red-600 text-white hover:bg-red-700 active:bg-red-800 transition-colors font-semibold text-lg flex items-center justify-center gap-2"
                  >
                    <span className="text-2xl">⏹️</span>
                    Stop Listening
                  </button>
                )}
              </div>

              {/* Reset Button */}
              <button
                onClick={handleReset}
                disabled={isProcessing}
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
                <div className="mt-4 p-4 bg-gradient-to-br from-green-50 to-blue-50 rounded-lg">
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
                    <div className="text-4xl mb-4">🎤</div>
                    <div>Click "Start Listening" to begin</div>
                    <div className="text-sm mt-2">VAD will automatically detect when you speak</div>
                  </div>
                )}

                {messages.map((message) => (
                  <div
                    key={message.id}
                    className={`p-4 rounded-lg ${
                      message.role === 'user'
                        ? 'bg-green-50 border border-green-200'
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
                        <div className="text-gray-700">{message.text}</div>
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
            VAD-powered conversation | Silero VAD + LFM2-Audio + Kokoro TTS
          </p>
        </footer>
      </div>
    </main>
  );
}
