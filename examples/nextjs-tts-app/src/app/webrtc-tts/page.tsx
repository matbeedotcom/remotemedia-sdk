'use client';

/**
 * WebRTC + Pipeline TTS Demo with Bidirectional Audio
 *
 * This page demonstrates:
 * 1. WebRTC signaling via gRPC service
 * 2. Real-time bidirectional audio pipeline execution
 * 3. Microphone input → VAD/STT pipeline
 * 4. TTS pipeline → Audio output
 *
 * Architecture:
 *   Browser <-> WebRTC <-> RemoteMedia Server <-> Pipeline (VAD/STT/TTS)
 *   (signaling via gRPC)      (media via RTP)
 */

import { useEffect, useState, useRef } from 'react';
import { encodeTextData } from '@/lib/proto-utils';

interface PeerConnection {
  pc: RTCPeerConnection;
  dataChannel: RTCDataChannel | null;
  localStream: MediaStream | null;
}

export default function WebRTCTTSPage() {
  // State
  const [text, setText] = useState('');
  const [status, setStatus] = useState<string>('Disconnected');
  const [isConnected, setIsConnected] = useState(false);
  const [isSpeaking, setIsSpeaking] = useState(false);
  const [isListening, setIsListening] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Refs
  const peerConnectionRef = useRef<PeerConnection | null>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const remoteStreamRef = useRef<MediaStream | null>(null);
  const localStreamRef = useRef<MediaStream | null>(null);

  // WebRTC Configuration
  const rtcConfig: RTCConfiguration = {
    iceServers: [
      { urls: 'stun:stun.l.google.com:19302' },
      { urls: 'stun:stun1.l.google.com:19302' },
    ],
  };

  /**
   * Connect to WebRTC server via gRPC signaling
   */
  async function connectWebRTC() {
    try {
      setStatus('Connecting...');
      setError(null);

      // Request microphone access
      let localStream: MediaStream | null = null;
      try {
        localStream = await navigator.mediaDevices.getUserMedia({
          audio: {
            echoCancellation: true,
            noiseSuppression: true,
            autoGainControl: true,
          },
          video: false,
        });
        console.log('Microphone access granted');
        localStreamRef.current = localStream;
        setIsListening(true);
      } catch (micError) {
        console.warn('Microphone access denied:', micError);
        setError('Microphone access denied. Text-only mode enabled.');
        // Continue without microphone - text input will still work
      }

      // Create peer connection
      const pc = new RTCPeerConnection(rtcConfig);

      // Add local audio track if available (microphone input for VAD/STT)
      if (localStream) {
        localStream.getTracks().forEach((track) => {
          pc.addTrack(track, localStream);
          console.log('Added local track:', track.kind);
        });
      }

      // Create data channel for text input
      const dataChannel = pc.createDataChannel('tts-input', {
        ordered: true,
      });

      // Handle data channel events
      dataChannel.onopen = () => {
        console.log('Data channel opened');
        setStatus('Connected');
        setIsConnected(true);
      };

      dataChannel.onclose = () => {
        console.log('Data channel closed');
        setStatus('Disconnected');
        setIsConnected(false);
      };

      dataChannel.onerror = (event) => {
        console.error('Data channel error:', event);
        setError('Data channel error');
      };

      // Handle incoming audio stream (TTS output from server)
      pc.ontrack = (event) => {
        console.log('Received remote track:', event.track.kind);

        if (event.track.kind === 'audio') {
          const stream = event.streams[0];
          remoteStreamRef.current = stream;

          if (audioRef.current) {
            audioRef.current.srcObject = stream;
            audioRef.current.play().catch((e) => {
              console.error('Error playing audio:', e);
              setError('Failed to play audio. Please check browser permissions.');
            });
          }
        }
      };

      // Handle ICE candidates
      pc.onicecandidate = async (event) => {
        if (event.candidate) {
          console.log('New ICE candidate:', event.candidate.candidate);

          // Send ICE candidate to signaling server
          await sendIceCandidateToServer(event.candidate);
        }
      };

      // Handle connection state
      pc.onconnectionstatechange = () => {
        console.log('Connection state:', pc.connectionState);
        setStatus(`Connection: ${pc.connectionState}`);

        if (pc.connectionState === 'failed' || pc.connectionState === 'disconnected') {
          setIsConnected(false);
          setError('Connection failed or disconnected');
        }
      };

      // Create offer
      const offer = await pc.createOffer({
        offerToReceiveAudio: true,
        offerToReceiveVideo: false,
      });

      await pc.setLocalDescription(offer);

      // Send offer to signaling server and get answer
      const answer = await sendOfferToServer(offer);

      await pc.setRemoteDescription(new RTCSessionDescription(answer));

      // Save connection
      peerConnectionRef.current = {
        pc,
        dataChannel,
        localStream,
      };

      setStatus('Connected');
      setIsConnected(true);
    } catch (error) {
      console.error('Error connecting:', error);
      setError(error instanceof Error ? error.message : 'Connection failed');
      setStatus('Error');

      // Clean up local stream on error
      if (localStreamRef.current) {
        localStreamRef.current.getTracks().forEach((track) => track.stop());
        localStreamRef.current = null;
      }
    }
  }

  /**
   * Send offer to gRPC signaling server
   */
  async function sendOfferToServer(
    offer: RTCSessionDescriptionInit
  ): Promise<RTCSessionDescriptionInit> {
    const response = await fetch('/api/webrtc/signal', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        type: 'offer',
        sdp: offer.sdp,
      }),
    });

    if (!response.ok) {
      throw new Error(`Signaling failed: ${response.statusText}`);
    }

    const data = await response.json();
    return data.answer;
  }

  /**
   * Send ICE candidate to signaling server
   */
  async function sendIceCandidateToServer(candidate: RTCIceCandidate): Promise<void> {
    await fetch('/api/webrtc/signal', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        type: 'ice-candidate',
        candidate: candidate.candidate,
        sdpMid: candidate.sdpMid,
        sdpMLineIndex: candidate.sdpMLineIndex,
      }),
    });
  }

  /**
   * Send text to TTS pipeline via data channel
   */
  async function sendTextToTTS() {
    if (!peerConnectionRef.current?.dataChannel) {
      setError('Not connected');
      return;
    }

    if (!text.trim()) {
      setError('Please enter some text');
      return;
    }

    try {
      setIsSpeaking(true);
      setError(null);

      // Encode text as Protobuf DataBuffer (binary RuntimeData)
      const protobufData = encodeTextData(text.trim());
      peerConnectionRef.current.dataChannel.send(protobufData);

      console.log('Sent text to TTS pipeline as Protobuf DataBuffer');
    } catch (error) {
      console.error('Error sending text:', error);
      setError(error instanceof Error ? error.message : 'Failed to send text');
      setIsSpeaking(false);
    }
  }

  /**
   * Disconnect from WebRTC
   */
  function disconnect() {
    if (peerConnectionRef.current) {
      // Stop local stream (microphone)
      if (peerConnectionRef.current.localStream) {
        peerConnectionRef.current.localStream.getTracks().forEach((track) => track.stop());
      }

      peerConnectionRef.current.dataChannel?.close();
      peerConnectionRef.current.pc.close();
      peerConnectionRef.current = null;
    }

    if (localStreamRef.current) {
      localStreamRef.current.getTracks().forEach((track) => track.stop());
      localStreamRef.current = null;
    }

    if (remoteStreamRef.current) {
      remoteStreamRef.current.getTracks().forEach((track) => track.stop());
      remoteStreamRef.current = null;
    }

    if (audioRef.current) {
      audioRef.current.srcObject = null;
    }

    setIsConnected(false);
    setIsListening(false);
    setStatus('Disconnected');
    setIsSpeaking(false);
  }

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      disconnect();
    };
  }, []);

  // Listen for audio end
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    const handleEnded = () => {
      setIsSpeaking(false);
    };

    audio.addEventListener('ended', handleEnded);
    return () => {
      audio.removeEventListener('ended', handleEnded);
    };
  }, []);

  return (
    <div className="min-h-screen bg-gradient-to-br from-purple-50 to-blue-50 p-8">
      <div className="max-w-4xl mx-auto">
        {/* Header */}
        <div className="bg-white rounded-lg shadow-lg p-6 mb-6">
          <h1 className="text-3xl font-bold text-gray-800 mb-2">
            WebRTC + Bidirectional Audio Pipeline Demo
          </h1>
          <p className="text-gray-600">
            Real-time bidirectional audio via WebRTC with VAD, STT, and TTS pipelines
          </p>
        </div>

        {/* Connection Status */}
        <div className="bg-white rounded-lg shadow-lg p-6 mb-6">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-xl font-semibold text-gray-800 mb-2">Connection Status</h2>
              <p className="text-lg mb-2">
                Status:{' '}
                <span
                  className={`font-semibold ${
                    isConnected ? 'text-green-600' : 'text-gray-600'
                  }`}
                >
                  {status}
                </span>
              </p>
              {isConnected && (
                <p className="text-sm text-gray-600">
                  Microphone:{' '}
                  <span className={`font-semibold ${isListening ? 'text-green-600' : 'text-gray-500'}`}>
                    {isListening ? '🎤 Active' : 'Disabled'}
                  </span>
                </p>
              )}
            </div>

            <div className="flex gap-3">
              {!isConnected ? (
                <button
                  onClick={connectWebRTC}
                  className="px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors disabled:bg-gray-400"
                  disabled={status === 'Connecting...'}
                >
                  Connect
                </button>
              ) : (
                <button
                  onClick={disconnect}
                  className="px-6 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors"
                >
                  Disconnect
                </button>
              )}
            </div>
          </div>
        </div>

        {/* Error Display */}
        {error && (
          <div className="bg-red-50 border border-red-200 rounded-lg p-4 mb-6">
            <p className="text-red-800">
              <strong>Error:</strong> {error}
            </p>
          </div>
        )}

        {/* Text Input */}
        <div className="bg-white rounded-lg shadow-lg p-6 mb-6">
          <h2 className="text-xl font-semibold text-gray-800 mb-4">Enter Text</h2>

          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Enter text to synthesize..."
            className="w-full h-40 p-4 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent resize-none"
            disabled={!isConnected}
          />

          <div className="mt-4 flex justify-between items-center">
            <span className="text-sm text-gray-600">{text.length} / 10000 characters</span>

            <button
              onClick={sendTextToTTS}
              className="px-8 py-3 bg-green-600 text-white rounded-lg hover:bg-green-700 transition-colors disabled:bg-gray-400 disabled:cursor-not-allowed"
            >
              {isSpeaking ? 'Speaking...' : '🔊 Speak'}
            </button>
          </div>
        </div>

        {/* Audio Player (hidden) */}
        <audio ref={audioRef} hidden />

        {/* How It Works */}
        <div className="bg-white rounded-lg shadow-lg p-6">
          <h2 className="text-xl font-semibold text-gray-800 mb-4">How It Works</h2>

          <div className="space-y-3 text-gray-700">
            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-sm">
                1
              </span>
              <p>
                <strong>Connect:</strong> Establish WebRTC connection with microphone access
                via gRPC signaling
              </p>
            </div>

            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-green-600 text-white rounded-full flex items-center justify-center text-sm">
                2a
              </span>
              <p>
                <strong>Microphone Input:</strong> Your voice → WebRTC audio track → Server
                → VAD/STT pipeline (real-time voice detection)
              </p>
            </div>

            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-purple-600 text-white rounded-full flex items-center justify-center text-sm">
                2b
              </span>
              <p>
                <strong>Text Input:</strong> Send text via WebRTC data channel to TTS pipeline
              </p>
            </div>

            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-sm">
                3
              </span>
              <p>
                <strong>Process:</strong> Server executes pipelines (VAD, STT, TTS) in parallel
              </p>
            </div>

            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-sm">
                4
              </span>
              <p>
                <strong>Audio Output:</strong> TTS audio streamed back via WebRTC (Opus/RTP)
              </p>
            </div>

            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-sm">
                5
              </span>
              <p>
                <strong>Play:</strong> Browser receives and plays synthesized speech in real-time
              </p>
            </div>
          </div>

          <div className="mt-6 p-4 bg-blue-50 rounded-lg">
            <h3 className="font-semibold text-blue-900 mb-2">Technical Details</h3>
            <ul className="text-sm text-blue-800 space-y-1">
              <li>• Signaling: gRPC bidirectional streaming</li>
              <li>• Media: WebRTC with Opus codec (bidirectional audio)</li>
              <li>• Input: Microphone (VAD/STT) + Data Channel (Text)</li>
              <li>• Output: Audio track (TTS synthesis)</li>
              <li>• Pipeline: RemoteMedia runtime with VAD, STT, and Kokoro TTS</li>
              <li>• Latency: ~50-100ms for real-time bidirectional streaming</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}
