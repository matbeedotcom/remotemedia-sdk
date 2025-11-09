/**
 * Speech-to-Speech Streaming Client
 *
 * Client-side library for real-time conversational AI using audio input/output.
 * Handles audio recording, streaming to server, and playback of responses.
 */

export interface S2SStreamOptions {
  /** API endpoint (default: /api/s2s/stream) */
  apiEndpoint?: string;
  /** Session ID for conversation continuity */
  sessionId?: string;
  /** System prompt override */
  systemPrompt?: string;
  /** Sample rate for audio (default: 24000) */
  sampleRate?: number;
  /** Reset conversation history */
  reset?: boolean;
}

export interface S2SCallbacks {
  /** Called when text response is received */
  onText?: (text: string) => void;
  /** Called when audio chunk is received */
  onAudio?: (audioData: Float32Array, sampleRate: number) => void;
  /** Called when metrics are received */
  onMetrics?: (metrics: S2SMetrics) => void;
  /** Called when stream starts */
  onStart?: () => void;
  /** Called when stream completes */
  onComplete?: (sessionId: string) => void;
  /** Called on error */
  onError?: (error: Error) => void;
}

export interface S2SMetrics {
  sessionId: string;
  cacheHits?: number;
  cacheMisses?: number;
  cachedNodesCount?: number;
  cacheHitRate?: number;
  averageLatencyMs?: number;
}

export interface S2SStreamResponse {
  type: 'text' | 'audio' | 'metrics' | 'complete' | 'error';
  sequence: number;
  content?: string | object;
  sampleRate?: number;
  numSamples?: number;
  sessionId?: string;
}

/**
 * Stream audio to the S2S API and receive text + audio responses
 */
export async function streamS2S(
  audioData: Float32Array,
  options: S2SStreamOptions = {},
  callbacks: S2SCallbacks = {}
): Promise<string> {
  const {
    apiEndpoint = '/api/s2s/stream',
    sessionId,
    systemPrompt,
    sampleRate = 24000,
    reset = false,
  } = options;

  const { onText, onAudio, onMetrics, onStart, onComplete, onError } = callbacks;

  try {
    onStart?.();

    // Convert Float32Array to base64-encoded buffer
    const audioBuffer = Buffer.from(audioData.buffer);
    const audioBase64 = audioBuffer.toString('base64');

    // Make request
    const response = await fetch(apiEndpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        audio: audioBase64,
        sampleRate,
        sessionId,
        systemPrompt,
        reset,
      }),
    });

    if (!response.ok) {
      throw new Error(`API error: ${response.status} ${response.statusText}`);
    }

    const returnedSessionId = response.headers.get('X-Session-ID') || sessionId || 'unknown';

    // Process streaming response
    const reader = response.body?.getReader();
    if (!reader) {
      throw new Error('No response body');
    }

    const decoder = new TextDecoder();
    let textBuffer = '';
    let fullTextResponse = '';

    while (true) {
      const { done, value } = await reader.read();

      if (done) {
        break;
      }

      // Decode chunk and add to buffer
      textBuffer += decoder.decode(value, { stream: true });

      // Process complete lines (newline-delimited JSON)
      const lines = textBuffer.split('\n');
      const incompleteLine = lines.pop() || '';
      textBuffer = incompleteLine; // Keep incomplete line in buffer

      for (const line of lines) {
        if (!line.trim()) continue;

        try {
          const message: S2SStreamResponse = JSON.parse(line);

          switch (message.type) {
            case 'text':
              if (typeof message.content === 'string') {
                fullTextResponse += message.content;
                onText?.(message.content);
              }
              break;

            case 'audio':
              if (typeof message.content === 'string') {
                // Decode base64 audio
                const audioBuffer = Buffer.from(message.content, 'base64');
                const float32Array = new Float32Array(
                  audioBuffer.buffer,
                  audioBuffer.byteOffset,
                  audioBuffer.byteLength / 4
                );
                onAudio?.(float32Array, message.sampleRate || 24000);
              }
              break;

            case 'metrics':
              if (typeof message.content === 'object') {
                onMetrics?.(message.content as S2SMetrics);
              }
              break;

            case 'complete':
              onComplete?.(returnedSessionId);
              break;

            case 'error':
              const errorMsg = typeof message.content === 'string' ? message.content : 'Unknown error';
              throw new Error(errorMsg);
          }
        } catch (parseError) {
          console.error('Failed to parse S2S message:', line, parseError);
        }
      }
    }

    return fullTextResponse;
  } catch (error) {
    const err = error instanceof Error ? error : new Error(String(error));
    onError?.(err);
    throw err;
  }
}

/**
 * Record audio from microphone and stream to S2S
 */
export class S2SAudioRecorder {
  private mediaStream: MediaStream | null = null;
  private mediaRecorder: MediaRecorder | null = null;
  private audioChunks: Blob[] = [];
  private audioContext: AudioContext | null = null;

  /**
   * Start recording audio from microphone
   */
  async startRecording(): Promise<void> {
    try {
      // Request microphone access
      this.mediaStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          sampleRate: 24000,
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });

      // Create media recorder
      this.mediaRecorder = new MediaRecorder(this.mediaStream, {
        mimeType: 'audio/webm;codecs=opus',
      });

      this.audioChunks = [];

      this.mediaRecorder.ondataavailable = (event) => {
        if (event.data.size > 0) {
          this.audioChunks.push(event.data);
        }
      };

      this.mediaRecorder.start();
      console.log('[S2SRecorder] Recording started');
    } catch (error) {
      console.error('[S2SRecorder] Failed to start recording:', error);
      throw error;
    }
  }

  /**
   * Stop recording and return recorded audio as Float32Array
   */
  async stopRecording(): Promise<Float32Array> {
    return new Promise((resolve, reject) => {
      if (!this.mediaRecorder || this.mediaRecorder.state === 'inactive') {
        reject(new Error('MediaRecorder not active'));
        return;
      }

      this.mediaRecorder.onstop = async () => {
        try {
          // Create blob from chunks
          const audioBlob = new Blob(this.audioChunks, { type: 'audio/webm;codecs=opus' });

          // Decode to PCM
          const float32Array = await this.decodeAudioBlob(audioBlob);
          resolve(float32Array);
        } catch (error) {
          reject(error);
        } finally {
          // Clean up
          this.cleanup();
        }
      };

      this.mediaRecorder.stop();
      console.log('[S2SRecorder] Recording stopped');
    });
  }

  /**
   * Decode audio blob to Float32Array PCM
   */
  private async decodeAudioBlob(blob: Blob): Promise<Float32Array> {
    // Create audio context if needed
    if (!this.audioContext) {
      this.audioContext = new AudioContext({ sampleRate: 24000 });
    }

    // Convert blob to array buffer
    const arrayBuffer = await blob.arrayBuffer();

    // Decode audio data
    const audioBuffer = await this.audioContext.decodeAudioData(arrayBuffer);

    // Get first channel as Float32Array
    const channelData = audioBuffer.getChannelData(0);

    // Resample to 24kHz if needed
    if (audioBuffer.sampleRate !== 24000) {
      return await this.resampleAudio(channelData, audioBuffer.sampleRate, 24000);
    }

    return channelData;
  }

  /**
   * Simple linear resampling
   */
  private async resampleAudio(
    input: Float32Array,
    inputRate: number,
    outputRate: number
  ): Promise<Float32Array> {
    const ratio = inputRate / outputRate;
    const outputLength = Math.floor(input.length / ratio);
    const output = new Float32Array(outputLength);

    for (let i = 0; i < outputLength; i++) {
      const srcIndex = i * ratio;
      const srcIndexFloor = Math.floor(srcIndex);
      const srcIndexCeil = Math.min(srcIndexFloor + 1, input.length - 1);
      const t = srcIndex - srcIndexFloor;

      // Linear interpolation
      output[i] = input[srcIndexFloor] * (1 - t) + input[srcIndexCeil] * t;
    }

    return output;
  }

  /**
   * Cancel recording
   */
  cancel(): void {
    if (this.mediaRecorder && this.mediaRecorder.state !== 'inactive') {
      this.mediaRecorder.stop();
    }
    this.cleanup();
  }

  /**
   * Check if currently recording
   */
  isRecording(): boolean {
    return this.mediaRecorder !== null && this.mediaRecorder.state === 'recording';
  }

  /**
   * Clean up resources
   */
  private cleanup(): void {
    if (this.mediaStream) {
      this.mediaStream.getTracks().forEach((track) => track.stop());
      this.mediaStream = null;
    }
    this.mediaRecorder = null;
    this.audioChunks = [];
  }
}

/**
 * Get session info
 */
export async function getSessionInfo(sessionId: string): Promise<any> {
  const response = await fetch(`/api/s2s/stream?sessionId=${encodeURIComponent(sessionId)}`);

  if (!response.ok) {
    throw new Error(`Failed to get session info: ${response.status}`);
  }

  return response.json();
}

/**
 * Delete/reset session
 */
export async function deleteSession(sessionId: string): Promise<void> {
  const response = await fetch(
    `/api/s2s/stream?sessionId=${encodeURIComponent(sessionId)}`,
    {
      method: 'DELETE',
    }
  );

  if (!response.ok) {
    throw new Error(`Failed to delete session: ${response.status}`);
  }
}
