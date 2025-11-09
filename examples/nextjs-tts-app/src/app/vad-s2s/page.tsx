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

  // Debug controls
  const [debugSampleRate, setDebugSampleRate] = useState<number>(24000);
  const [debugShowPanel, setDebugShowPanel] = useState<boolean>(false);
  const [debugAudioFormat, setDebugAudioFormat] = useState<'float32' | 'int16' | 'uint8'>('float32');
  const [debugChunksReceived, setDebugChunksReceived] = useState<number>(0);
  const [debugLastChunkSize, setDebugLastChunkSize] = useState<number>(0);

  // Refs
  const audioContextRef = useRef<AudioContext | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const inputWorkletNodeRef = useRef<AudioWorkletNode | null>(null);
  const sessionInitializedRef = useRef(false);
  const isActiveRef = useRef(false);
  const resultsAbortControllerRef = useRef<AbortController | null>(null);
  const audioBufferRef = useRef<Float32Array>(new Float32Array(0));
  const currentMessageIdRef = useRef<string | null>(null);
  const playbackContextRef = useRef<AudioContext | null>(null);

  // Audio playback with AudioWorklet
  const audioWorkletNodeRef = useRef<AudioWorkletNode | null>(null);
  const audioQueueRef = useRef<Float32Array[]>([]);
  const currentSampleRateRef = useRef<number>(24000);

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
          systemPrompt: 'Respond with interleaved text and audio.',
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

            // Don't start separate results listener - results come through this same connection
            // Continue processing the stream for results
          }

          // Handle all result types through the same connection
          handleStreamResponse(data);
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

      // Create audio context - let it match the MediaStream's actual sample rate
      // (browser may ignore our 16kHz request and use 48kHz or other rates)
      const audioContext = new AudioContext();
      audioContextRef.current = audioContext;

      const source = audioContext.createMediaStreamSource(stream);

      console.log(`[VAD S2S] Microphone AudioContext sample rate: ${audioContext.sampleRate}Hz`);

      // Load AudioWorklet for input processing (replaces deprecated ScriptProcessorNode)
      await audioContext.audioWorklet.addModule('/audio-input-processor.js');

      // Create input processor worklet with sample rate
      const inputWorklet = new AudioWorkletNode(audioContext, 'audio-input-processor', {
        processorOptions: {
          sampleRate: audioContext.sampleRate
        }
      });
      inputWorkletNodeRef.current = inputWorklet;

      // Listen for audio chunks from the worklet
      inputWorklet.port.onmessage = (event) => {
        if (event.data.type === 'audiodata' && isActiveRef.current) {
          const audioChunk = event.data.samples;

          // Send chunk to backend via VAD pipeline (don't await - fire and forget)
          sendAudioChunk(audioChunk).catch((err) => {
            console.error('[VAD S2S] Failed to send chunk:', err);
          });
        }
      };

      // Connect source -> worklet (no need to connect to destination for input processing)
      source.connect(inputWorklet);

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

    // Disconnect input worklet
    if (inputWorkletNodeRef.current) {
      inputWorkletNodeRef.current.disconnect();
      inputWorkletNodeRef.current = null;
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

    // Clear audio queue and stop playback
    audioQueueRef.current = [];

    // Close audio worklet and context
    if (audioWorkletNodeRef.current) {
      audioWorkletNodeRef.current.disconnect();
      audioWorkletNodeRef.current = null;
    }
    if (playbackContextRef.current) {
      playbackContextRef.current.close();
      playbackContextRef.current = null;
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
      // Use the actual AudioContext sample rate (browser may not honor our 16kHz request)
      const actualSampleRate = audioContextRef.current?.sampleRate || 16000;

      const response = await fetch('/api/s2s/vad-stream', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          audio: base64Audio,
          sampleRate: actualSampleRate,
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
        // Check for text end marker
        if (data.content === '<|text_end|>') {
          console.log('[VAD S2S] Text generation ended');
          // Reset message ID so next text starts a new message
          currentMessageIdRef.current = null;
        } else {
          // Check if this is a new message or continuation
          if (!currentMessageIdRef.current) {
            // First text of a new response - create new message
            const messageId = `assistant_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
            currentMessageIdRef.current = messageId;

            console.log(`[VAD S2S] NEW MESSAGE started: "${data.content}"`);

            setMessages((prev) => [
              ...prev,
              {
                id: messageId,
                role: 'assistant',
                text: data.content,
                timestamp: new Date(),
              },
            ]);

            // Only reset audio for the FIRST text of a new message
            audioBufferRef.current = new Float32Array(0);
            audioQueueRef.current = [];

            if (audioWorkletNodeRef.current) {
              audioWorkletNodeRef.current.port.postMessage({ type: 'clear' });
            }
          } else {
            // Continuation of existing message - append text
            console.log(`[VAD S2S] Appending text to message: "${data.content}"`);

            setMessages((prev) => {
              const msgIndex = prev.findIndex(m => m.id === currentMessageIdRef.current);
              if (msgIndex !== -1) {
                const updated = [...prev];
                updated[msgIndex] = {
                  ...updated[msgIndex],
                  text: (updated[msgIndex].text || '') + data.content
                };
                return updated;
              }
              return prev;
            });
          }
        }

        setIsProcessing(false);
        break;

      case 'audio':
        // Decode base64 to audio array
        console.log(`[VAD S2S] Received audio chunk: ${data.content.length} base64 chars`);

        // Decode base64 to binary
        const binaryString = atob(data.content);
        const bytes = new Uint8Array(binaryString.length);
        for (let i = 0; i < binaryString.length; i++) {
          bytes[i] = binaryString.charCodeAt(i);
        }

        // Convert bytes to audio array based on debug format
        let float32Audio: Float32Array;
        switch (debugAudioFormat) {
          case 'float32':
            float32Audio = new Float32Array(bytes.buffer);
            break;
          case 'int16':
            // Convert int16 to float32
            const int16Data = new Int16Array(bytes.buffer);
            float32Audio = new Float32Array(int16Data.length);
            for (let i = 0; i < int16Data.length; i++) {
              float32Audio[i] = int16Data[i] / 32768.0; // Normalize to [-1, 1]
            }
            break;
          case 'uint8':
            // Convert uint8 to float32
            float32Audio = new Float32Array(bytes.length);
            for (let i = 0; i < bytes.length; i++) {
              float32Audio[i] = (bytes[i] - 128) / 128.0; // Normalize to [-1, 1]
            }
            break;
        }

        console.log(`[VAD S2S] Decoded audio (${debugAudioFormat}): ${float32Audio.length} samples`);

        // Update debug metrics
        setDebugChunksReceived(prev => prev + 1);
        setDebugLastChunkSize(float32Audio.length);

        // Concatenate with existing buffer for display purposes
        const prevBuffer = audioBufferRef.current;
        const newBuffer = new Float32Array(prevBuffer.length + float32Audio.length);
        newBuffer.set(prevBuffer);
        newBuffer.set(float32Audio, prevBuffer.length);
        audioBufferRef.current = newBuffer;

        // Update the message with accumulated audio (for display purposes)
        if (currentMessageIdRef.current) {
          setMessages((prev) => {
            const msgIndex = prev.findIndex(m => m.id === currentMessageIdRef.current);
            if (msgIndex !== -1) {
              const msg = prev[msgIndex];
              return [
                ...prev.slice(0, msgIndex),
                { ...msg, audio: audioBufferRef.current },
                ...prev.slice(msgIndex + 1)
              ];
            }
            return prev;
          });
        }

        // Add to audio queue for buffered playback
        // Use debug sample rate
        currentSampleRateRef.current = debugSampleRate;
        audioQueueRef.current.push(float32Audio);
        console.log(`[VAD S2S] Audio queued: ${audioQueueRef.current.length} chunks waiting`);

        // Process queue immediately for low latency
        processAudioQueue();
        break;

      case 'json':
        console.log('[VAD S2S] JSON output:', data.content);
        break;

      case 'metrics':
        setMetrics(data.content);
        break;

      case 'complete':
        setIsProcessing(false);
        // Note: Message ID reset is now handled by <|text_end|> marker

        // Process any remaining audio in the queue
        if (audioQueueRef.current.length > 0) {
          console.log('[VAD S2S] Processing remaining audio chunks on complete');
          processAudioQueue();
        }
        break;

      case 'error':
        setError(data.content);
        setIsProcessing(false);
        currentMessageIdRef.current = null;
        break;
    }
  };

  /**
   * Initialize AudioWorklet for streaming playback
   */
  const initializeAudioWorklet = async () => {
    const SAMPLE_RATE = debugSampleRate;

    // Create audio context if needed
    if (!playbackContextRef.current) {
      playbackContextRef.current = new AudioContext({ sampleRate: SAMPLE_RATE });

      try {
        // Load the enhanced AudioWorklet processor with better buffering
        await playbackContextRef.current.audioWorklet.addModule('/audio-stream-processor-v2.js');

        // Create AudioWorkletNode with enhanced buffering and smoothing
        audioWorkletNodeRef.current = new AudioWorkletNode(
          playbackContextRef.current,
          'audio-stream-processor-v2'
        );

        // Connect to speakers
        audioWorkletNodeRef.current.connect(playbackContextRef.current.destination);

        // Listen for status updates from enhanced processor
        audioWorkletNodeRef.current.port.onmessage = (event) => {
          if (event.data.type === 'status') {
            // Enhanced processor status with dynamic metrics
            const status = [];
            const sampleRate = debugSampleRate;
            status.push(`[VAD S2S] Audio Buffer Status:`);
            status.push(`  - Buffered: ${event.data.bufferedSamples} samples (${(event.data.bufferedSamples / sampleRate * 1000).toFixed(1)}ms)`);
            status.push(`  - Target: ${event.data.targetBufferSize} samples (${(event.data.targetBufferSize / sampleRate * 1000).toFixed(1)}ms)`);

            if (event.data.lowWaterMark) {
              status.push(`  - Low Water Mark: ${event.data.lowWaterMark} samples (${(event.data.lowWaterMark / sampleRate * 1000).toFixed(1)}ms)`);
            }

            status.push(`  - Buffer Health: ${(event.data.bufferHealth * 100).toFixed(1)}%`);
            status.push(`  - Playing: ${event.data.isPlaying}`);
            status.push(`  - Underruns: ${event.data.underruns}`);

            if (event.data.averageChunkInterval) {
              status.push(`  - Avg Chunk Interval: ${event.data.averageChunkInterval.toFixed(1)}ms`);
            }
            if (event.data.averageChunkSize) {
              status.push(`  - Avg Chunk Size: ${event.data.averageChunkSize.toFixed(0)} samples (${(event.data.averageChunkSize / sampleRate * 1000).toFixed(1)}ms)`);
            }

            console.log(status.join('\n'));
          } else if (event.data.type === 'bufferStatus') {
            // Legacy status (fallback)
            console.log(`[VAD S2S] Buffer: ${event.data.bufferedSamples} samples`);
          }
        };

        console.log('[VAD S2S] AudioWorklet initialized');
      } catch (err) {
        console.error('[VAD S2S] Failed to initialize AudioWorklet:', err);
      }
    }

    return audioWorkletNodeRef.current;
  };

  /**
   * Process audio queue - send chunks to AudioWorklet
   */
  const processAudioQueue = async () => {
    if (audioQueueRef.current.length === 0) {
      return;
    }

    // Initialize AudioWorklet if needed
    let workletNode = audioWorkletNodeRef.current;
    if (!workletNode) {
      workletNode = await initializeAudioWorklet();
      if (!workletNode) {
        console.error('[VAD S2S] AudioWorklet not available');
        return;
      }
    }

    // Send all queued chunks to the AudioWorklet
    while (audioQueueRef.current.length > 0) {
      const audioData = audioQueueRef.current.shift()!;

      // Send audio data to worklet
      workletNode.port.postMessage({
        type: 'audio',
        samples: audioData
      });

      console.log(`[VAD S2S] Sent ${audioData.length} samples to AudioWorklet`);
    }
  };

  /**
   * Play audio from a message
   */
  const playMessageAudio = (audio: Float32Array) => {
    if (!audio || audio.length === 0) {
      console.warn('[VAD S2S] No audio to play');
      return;
    }
    console.log('[VAD S2S] Playing message audio...');
    try {
      // Create a temporary audio context for playback using debug sample rate
      const ctx = new AudioContext({ sampleRate: debugSampleRate });

      // Create audio buffer
      const audioBuffer = ctx.createBuffer(1, audio.length, debugSampleRate);

      // Create a new Float32Array to ensure proper ArrayBuffer type for TypeScript
      const audioData = new Float32Array(audio);
      audioBuffer.copyToChannel(audioData, 0);

      // Create source and play
      const source = ctx.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(ctx.destination);
      source.start();

      console.log(`[VAD S2S] Playing audio: ${audio.length} samples (${(audio.length / debugSampleRate).toFixed(2)}s) @ ${debugSampleRate}Hz`);

      // Clean up when done
      source.onended = () => {
        ctx.close();
      };
    } catch (err) {
      console.error('[VAD S2S] Failed to play audio:', err);
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

    // Clear audio state
    audioQueueRef.current = [];
    audioBufferRef.current = new Float32Array(0);
    currentMessageIdRef.current = null;

    // Reset debug counters
    setDebugChunksReceived(0);
    setDebugLastChunkSize(0);

    // Clear AudioWorklet and close playback context
    if (audioWorkletNodeRef.current) {
      audioWorkletNodeRef.current.port.postMessage({ type: 'clear' });
      audioWorkletNodeRef.current.disconnect();
      audioWorkletNodeRef.current = null;
    }
    if (playbackContextRef.current) {
      playbackContextRef.current.close();
      playbackContextRef.current = null;
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

              {/* Debug Panel */}
              <div className="mt-4 p-4 bg-gradient-to-br from-purple-50 to-pink-50 rounded-lg border border-purple-200">
                <button
                  onClick={() => setDebugShowPanel(!debugShowPanel)}
                  className="w-full flex items-center justify-between font-semibold text-gray-900 mb-2"
                >
                  <span>🔧 Debug Audio</span>
                  <span>{debugShowPanel ? '▼' : '▶'}</span>
                </button>

                {debugShowPanel && (
                  <div className="space-y-3 text-xs">
                    {/* Sample Rate */}
                    <div>
                      <label className="block font-semibold text-gray-700 mb-1">
                        Sample Rate (Hz)
                      </label>
                      <select
                        value={debugSampleRate}
                        onChange={(e) => setDebugSampleRate(Number(e.target.value))}
                        disabled={isActive}
                        className="w-full px-2 py-1 border border-gray-300 rounded text-sm disabled:opacity-50"
                      >
                        <option value={8000}>8000 Hz</option>
                        <option value={16000}>16000 Hz</option>
                        <option value={22050}>22050 Hz</option>
                        <option value={24000}>24000 Hz (Default)</option>
                        <option value={44100}>44100 Hz</option>
                        <option value={48000}>48000 Hz</option>
                      </select>
                      {isActive && (
                        <div className="text-xs text-orange-600 mt-1">
                          Stop to change
                        </div>
                      )}
                    </div>

                    {/* Audio Format */}
                    <div>
                      <label className="block font-semibold text-gray-700 mb-1">
                        Audio Format
                      </label>
                      <select
                        value={debugAudioFormat}
                        onChange={(e) => setDebugAudioFormat(e.target.value as 'float32' | 'int16' | 'uint8')}
                        className="w-full px-2 py-1 border border-gray-300 rounded text-sm"
                      >
                        <option value="float32">Float32 (Default)</option>
                        <option value="int16">Int16 PCM</option>
                        <option value="uint8">UInt8 PCM</option>
                      </select>
                    </div>

                    {/* Chunk Statistics */}
                    <div className="pt-2 border-t border-purple-200">
                      <div className="font-semibold text-gray-700 mb-1">Chunk Stats</div>
                      <div className="space-y-1 text-gray-600">
                        <div>
                          <strong>Chunks Received:</strong> {debugChunksReceived}
                        </div>
                        <div>
                          <strong>Last Chunk:</strong> {debugLastChunkSize} samples
                          {debugLastChunkSize > 0 && (
                            <span className="text-gray-500">
                              {' '}({(debugLastChunkSize / debugSampleRate * 1000).toFixed(1)}ms)
                            </span>
                          )}
                        </div>
                        <div>
                          <strong>Expected @24kHz:</strong> 1920 samples (80ms)
                        </div>
                      </div>
                    </div>

                    {/* Reset Button */}
                    <button
                      onClick={() => {
                        setDebugChunksReceived(0);
                        setDebugLastChunkSize(0);
                      }}
                      className="w-full px-2 py-1 bg-purple-100 hover:bg-purple-200 text-purple-700 rounded text-sm"
                    >
                      Reset Stats
                    </button>
                  </div>
                )}
              </div>
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
                          <button
                            onClick={() => playMessageAudio(message.audio!)}
                            className="mt-2 px-3 py-1 text-xs bg-blue-100 hover:bg-blue-200 active:bg-blue-300 text-blue-700 rounded-full transition-colors flex items-center gap-1"
                          >
                            <span>▶️</span>
                            Play Audio ({message.audio.length} samples, {(message.audio.length / debugSampleRate).toFixed(2)}s @ {debugSampleRate}Hz)
                          </button>
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
