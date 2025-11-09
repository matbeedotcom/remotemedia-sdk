'use client';

/**
 * Socket.io-based VAD Speech-to-Speech Demo
 *
 * Bidirectional Socket.io for real-time speech-to-speech:
 * - Send: Binary audio from microphone ('audio_input' event)
 * - Receive: Text events + Binary audio events (TTS output)
 */

import { useEffect, useRef, useState } from 'react';

interface Message {
  role: 'user' | 'assistant';
  text: string;
  audio?: Float32Array;
  timestamp: number;
}

export default function VADS2SWebSocketPage() {
  const [isListening, setIsListening] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]);
  const [status, setStatus] = useState('Disconnected');
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [currentStreamingText, setCurrentStreamingText] = useState('');

  // Refs
  const socketRef = useRef<WebSocket | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const inputWorkletNodeRef = useRef<AudioWorkletNode | null>(null);
  const streamRef = useRef<MediaStream | null>(null);

  // Audio playback
  const audioQueueRef = useRef<{ data: Float32Array; sampleRate: number }[]>([]);
  const isPlayingRef = useRef(false);

  const playNextAudio = async () => {
    if (isPlayingRef.current || audioQueueRef.current.length === 0) {
      return;
    }

    isPlayingRef.current = true;
    const { data: audioData, sampleRate } = audioQueueRef.current.shift()!;

    try {
      const ctx = new AudioContext({ sampleRate });
      const audioBuffer = ctx.createBuffer(1, audioData.length, sampleRate);
      audioBuffer.copyToChannel(audioData, 0);

      const source = ctx.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(ctx.destination);

      source.onended = () => {
        ctx.close();
        isPlayingRef.current = false;
        playNextAudio(); // Play next in queue
      };

      source.start();
    } catch (err) {
      console.error('[VAD S2S Socket.io] Failed to play audio:', err);
      isPlayingRef.current = false;
    }
  };

  const startListening = async () => {
    try {
      setStatus('Connecting...');

      // Create Socket.io connection
      const socket = new WebSocket('ws://' + window.location.host + '/api/s2s/socket');
      socketRef.current = socket;
      socket.onopen = () => {
        console.log('[VAD S2S Socket.io] Connected');
        setStatus('Initializing session...');

        // Send init event
        socket.send(JSON.stringify({
          type: 'init',
          payload: {
            systemPrompt: 'Respond with interleaved text and audio.',
            maxNewTokens: 512,
          },
        }));
      };
      socket.onmessage = (event) => {
        const data = JSON.parse(event.data);
        console.log('[VAD S2S Socket.io] Received message:', data.type);
        if (data.type === 'ready') {
          console.log('[VAD S2S Socket.io] Ready:', data);
          setSessionId(data.sessionId);
          setStatus('Ready - speak into microphone');
          startMicrophone();
        } else if (data.type === 'text_chunk') {
          // Streaming text chunk
          console.log('[VAD S2S Socket.io] Text chunk:', data.content);
          setCurrentStreamingText((prev) => prev + data.content);
        } else if (data.type === 'text_complete') {
          // Complete text message
          console.log('[VAD S2S Socket.io] Text complete:', data.content);
          setMessages((prev) => [
            ...prev,
            {
              role: 'assistant',
              text: data.content,
              timestamp: Date.now(),
            },
          ]);
          // Reset streaming text
          setCurrentStreamingText('');
        } else if (data.type === 'text') {
          // Legacy: handle old non-streaming text format
          console.log('[VAD S2S Socket.io] Text:', data);
          setMessages((prev) => [
            ...prev,
            {
              role: 'assistant',
              text: data.content,
              timestamp: Date.now(),
            },
          ]);
        } else if (data.type === 'error') {
          console.error('[VAD S2S Socket.io] Error:', data);
          setStatus(`Error: ${data.content}`);
        } else if (data.type === 'audio') {
          const audioData = new Float32Array(data.content);
          const sampleRate = data.sampleRate || 24000;
          console.log(`[VAD S2S Socket.io] Received audio: ${audioData.length} samples @ ${sampleRate}Hz`);
          // Add to playback queue with sample rate
          audioQueueRef.current.push({ data: audioData, sampleRate });
          playNextAudio();
        }
      };

      socket.onclose = () => {
        console.log('[VAD S2S Socket.io] Disconnected');
        setStatus('Disconnected');
        stopMicrophone();
      };

      setIsListening(true);
    } catch (error) {
      console.error('[VAD S2S Socket.io] Failed to start:', error);
      setStatus(`Error: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  };

  const startMicrophone = async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;

      const audioContext = new AudioContext();
      audioContextRef.current = audioContext;

      const source = audioContext.createMediaStreamSource(stream);

      // Load AudioWorklet
      await audioContext.audioWorklet.addModule('/audio-input-processor.js');

      const inputWorklet = new AudioWorkletNode(audioContext, 'audio-input-processor', {
        processorOptions: {
          sampleRate: audioContext.sampleRate,
        },
      });
      inputWorkletNodeRef.current = inputWorklet;

      inputWorklet.port.onmessage = (event) => {
        if (event.data.type === 'audiodata' && socketRef.current?.readyState === WebSocket.OPEN) {
          const audioChunk = event.data.samples;
          // Send as binary event
          const buffer = new Float32Array(audioChunk).buffer;
          socketRef.current.send(buffer);
        }
      };

      source.connect(inputWorklet);
      console.log('[VAD S2S Socket.io] Microphone started');
    } catch (error) {
      console.error('[VAD S2S Socket.io] Microphone error:', error);
      setStatus(`Microphone error: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  };

  const stopMicrophone = () => {
    if (inputWorkletNodeRef.current) {
      inputWorkletNodeRef.current.disconnect();
      inputWorkletNodeRef.current = null;
    }
    if (audioContextRef.current) {
      audioContextRef.current.close();
      audioContextRef.current = null;
    }
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
    }
  };

  const stopListening = () => {
    if (socketRef.current) {
      socketRef.current.send(JSON.stringify({ type: 'close' }));
      socketRef.current.close();
      socketRef.current = null;
    }
    stopMicrophone();
    setIsListening(false);
    setStatus('Disconnected');
  };

  useEffect(() => {
    return () => {
      stopListening();
    };
  }, []);

  return (
    <div className="min-h-screen bg-gray-50 p-8">
      <div className="max-w-4xl mx-auto">
        <h1 className="text-3xl font-bold mb-2">VAD Speech-to-Speech (Socket.io)</h1>
        <p className="text-gray-600 mb-6">
          Real-time conversational AI with VAD using Socket.io for efficient binary audio streaming
        </p>

        {/* Status */}
        <div className="bg-white rounded-lg shadow p-6 mb-6">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-lg font-semibold mb-2">Status</h2>
              <p className="text-gray-700">{status}</p>
              {sessionId && <p className="text-sm text-gray-500 mt-1">Session: {sessionId}</p>}
            </div>
            <button
              onClick={isListening ? stopListening : startListening}
              className={`px-6 py-3 rounded-lg font-semibold ${
                isListening
                  ? 'bg-red-500 hover:bg-red-600 text-white'
                  : 'bg-blue-500 hover:bg-blue-600 text-white'
              }`}
            >
              {isListening ? 'Stop Listening' : 'Start Listening'}
            </button>
          </div>
        </div>

        {/* Conversation */}
        <div className="bg-white rounded-lg shadow p-6">
          <h2 className="text-lg font-semibold mb-4">Conversation</h2>
          {messages.length === 0 && !currentStreamingText ? (
            <p className="text-gray-500 text-center py-8">
              No messages yet. Start listening and speak to begin.
            </p>
          ) : (
            <div className="space-y-4">
              {messages.map((message, idx) => (
                <div
                  key={idx}
                  className={`p-4 rounded-lg ${
                    message.role === 'user' ? 'bg-blue-50 ml-12' : 'bg-gray-50 mr-12'
                  }`}
                >
                  <div className="flex items-start justify-between">
                    <div className="flex-1">
                      <p className="font-semibold mb-1">
                        {message.role === 'user' ? 'You' : 'Assistant'}
                      </p>
                      <p className="text-gray-700">{message.text}</p>
                      <p className="text-xs text-gray-400 mt-2">
                        {new Date(message.timestamp).toLocaleTimeString()}
                      </p>
                    </div>
                  </div>
                </div>
              ))}
              {/* Show streaming text */}
              {currentStreamingText && (
                <div className="p-4 rounded-lg bg-gray-50 mr-12 border-2 border-blue-400 animate-pulse">
                  <div className="flex items-start justify-between">
                    <div className="flex-1">
                      <p className="font-semibold mb-1 text-blue-600">
                        Assistant (streaming...)
                      </p>
                      <p className="text-gray-700">{currentStreamingText}</p>
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
