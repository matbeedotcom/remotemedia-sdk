/**
 * VAD-based Continuous Audio Streaming Client
 *
 * Handles continuous audio streaming to the server with VAD-based speech detection.
 * Uses Web Audio API for recording and processing.
 */

export interface VADStreamOptions {
  /** API endpoint (default: /api/s2s/vad-stream) */
  apiEndpoint?: string;
  /** Session ID for conversation continuity */
  sessionId?: string;
  /** System prompt override */
  systemPrompt?: string;
  /** Sample rate (default: 24000) */
  sampleRate?: number;
  /** Chunk duration in milliseconds (default: 100) */
  chunkDurationMs?: number;
  /** VAD energy threshold (default: 0.02) */
  vadEnergyThreshold?: number;
}

export interface VADStreamCallbacks {
  /** Called when VAD detects speech */
  onSpeechStart?: () => void;
  /** Called when VAD detects silence */
  onSpeechEnd?: () => void;
  /** Called when text response is received */
  onText?: (text: string) => void;
  /** Called when audio response is received */
  onAudio?: (audioData: Float32Array, sampleRate: number) => void;
  /** Called when metrics are received */
  onMetrics?: (metrics: any) => void;
  /** Called when stream starts */
  onStart?: () => void;
  /** Called on error */
  onError?: (error: Error) => void;
}

/**
 * Continuous audio streaming client with VAD
 */
export class VADStreamingClient {
  private mediaStream: MediaStream | null = null;
  private audioContext: AudioContext | null = null;
  private scriptProcessor: ScriptProcessorNode | null = null;
  private isStreaming: boolean = false;
  private sessionId: string;
  private options: VADStreamOptions;
  private callbacks: VADStreamCallbacks;
  private eventSource: EventSource | null = null;

  // VAD state
  private isSpeechActive: boolean = false;
  private speechBuffer: Float32Array[] = [];

  constructor(options: VADStreamOptions = {}, callbacks: VADStreamCallbacks = {}) {
    this.options = {
      apiEndpoint: '/api/s2s/vad-stream',
      sampleRate: 24000,
      chunkDurationMs: 100,
      vadEnergyThreshold: 0.02,
      ...options,
    };
    this.callbacks = callbacks;
    this.sessionId =
      options.sessionId || `vad_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * Start continuous audio streaming
   */
  async start(): Promise<void> {
    if (this.isStreaming) {
      console.warn('[VAD Streaming] Already streaming');
      return;
    }

    try {
      this.callbacks.onStart?.();

      // Request microphone access
      this.mediaStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          sampleRate: this.options.sampleRate,
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });

      // Create audio context
      this.audioContext = new AudioContext({ sampleRate: this.options.sampleRate });
      const source = this.audioContext.createMediaStreamSource(this.mediaStream);

      // Create script processor for audio chunks
      const bufferSize = 4096; // Process in 4096 sample chunks
      this.scriptProcessor = this.audioContext.createScriptProcessor(bufferSize, 1, 1);

      this.scriptProcessor.onaudioprocess = (event) => {
        if (!this.isStreaming) return;

        const inputData = event.inputBuffer.getChannelData(0);
        this.processAudioChunk(new Float32Array(inputData));
      };

      // Connect nodes
      source.connect(this.scriptProcessor);
      this.scriptProcessor.connect(this.audioContext.destination);

      this.isStreaming = true;

      // Connect to SSE endpoint
      await this.connectToServer();

      console.log('[VAD Streaming] Started successfully');
    } catch (error) {
      console.error('[VAD Streaming] Failed to start:', error);
      this.cleanup();
      throw error;
    }
  }

  /**
   * Connect to server SSE endpoint
   */
  private async connectToServer(): Promise<void> {
    const endpoint = this.options.apiEndpoint!;

    // First, POST to establish session
    const response = await fetch(endpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        sessionId: this.sessionId,
        systemPrompt: this.options.systemPrompt,
      }),
    });

    if (!response.ok) {
      throw new Error(`Failed to connect to server: ${response.status}`);
    }

    // Set up SSE for receiving responses
    this.eventSource = new EventSource(`${endpoint}?sessionId=${this.sessionId}`);

    this.eventSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        this.handleServerMessage(data);
      } catch (error) {
        console.error('[VAD Streaming] Failed to parse server message:', error);
      }
    };

    this.eventSource.onerror = (error) => {
      console.error('[VAD Streaming] SSE error:', error);
      this.callbacks.onError?.(new Error('Server connection error'));
    };
  }

  /**
   * Process audio chunk through VAD
   */
  private processAudioChunk(audioData: Float32Array): void {
    // Calculate energy for VAD
    const energy = this.calculateEnergy(audioData);
    const hasSpeech = energy > (this.options.vadEnergyThreshold || 0.02);

    // Detect speech start/end
    if (hasSpeech && !this.isSpeechActive) {
      this.isSpeechActive = true;
      this.speechBuffer = [];
      this.callbacks.onSpeechStart?.();
      console.log('[VAD Streaming] Speech detected');
    } else if (!hasSpeech && this.isSpeechActive) {
      // Speech ended - send buffered audio to server
      this.isSpeechActive = false;
      this.callbacks.onSpeechEnd?.();
      console.log('[VAD Streaming] Speech ended');

      if (this.speechBuffer.length > 0) {
        this.sendSpeechSegment();
      }
    }

    // Buffer speech audio
    if (this.isSpeechActive) {
      this.speechBuffer.push(audioData);
    }
  }

  /**
   * Calculate audio energy for VAD
   */
  private calculateEnergy(audioData: Float32Array): number {
    let sum = 0;
    for (let i = 0; i < audioData.length; i++) {
      sum += audioData[i] * audioData[i];
    }
    return sum / audioData.length;
  }

  /**
   * Send buffered speech segment to server
   */
  private async sendSpeechSegment(): Promise<void> {
    if (this.speechBuffer.length === 0) return;

    // Concatenate buffer
    const totalLength = this.speechBuffer.reduce((sum, chunk) => sum + chunk.length, 0);
    const fullAudio = new Float32Array(totalLength);
    let offset = 0;
    for (const chunk of this.speechBuffer) {
      fullAudio.set(chunk, offset);
      offset += chunk.length;
    }

    console.log(`[VAD Streaming] Sending speech segment: ${fullAudio.length} samples`);

    // Convert to base64
    const buffer = Buffer.from(fullAudio.buffer);
    const audioBase64 = buffer.toString('base64');

    // Send to server
    try {
      await fetch(this.options.apiEndpoint!, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          sessionId: this.sessionId,
          audio: audioBase64,
          sampleRate: this.options.sampleRate,
        }),
      });
    } catch (error) {
      console.error('[VAD Streaming] Failed to send audio:', error);
      this.callbacks.onError?.(error as Error);
    }

    // Clear buffer
    this.speechBuffer = [];
  }

  /**
   * Handle messages from server
   */
  private handleServerMessage(data: any): void {
    switch (data.type) {
      case 'session':
        console.log(`[VAD Streaming] Session established: ${data.sessionId}`);
        break;

      case 'text':
        this.callbacks.onText?.(data.content);
        break;

      case 'audio':
        const audioBuffer = Buffer.from(data.content, 'base64');
        const float32Array = new Float32Array(
          audioBuffer.buffer,
          audioBuffer.byteOffset,
          audioBuffer.byteLength / 4
        );
        this.callbacks.onAudio?.(float32Array, data.sampleRate || 24000);
        break;

      case 'metrics':
        this.callbacks.onMetrics?.(data.content);
        break;

      case 'error':
        this.callbacks.onError?.(new Error(data.content));
        break;

      case 'info':
        console.log(`[VAD Streaming] Info: ${data.message}`);
        break;
    }
  }

  /**
   * Stop streaming
   */
  stop(): void {
    if (!this.isStreaming) {
      console.warn('[VAD Streaming] Not streaming');
      return;
    }

    this.isStreaming = false;

    // Send final speech segment if any
    if (this.isSpeechActive && this.speechBuffer.length > 0) {
      this.sendSpeechSegment();
    }

    this.cleanup();
    console.log('[VAD Streaming] Stopped');
  }

  /**
   * Cleanup resources
   */
  private cleanup(): void {
    if (this.scriptProcessor) {
      this.scriptProcessor.disconnect();
      this.scriptProcessor = null;
    }

    if (this.audioContext) {
      this.audioContext.close();
      this.audioContext = null;
    }

    if (this.mediaStream) {
      this.mediaStream.getTracks().forEach((track) => track.stop());
      this.mediaStream = null;
    }

    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }

    this.isSpeechActive = false;
    this.speechBuffer = [];
  }

  /**
   * Get current session ID
   */
  getSessionId(): string {
    return this.sessionId;
  }

  /**
   * Check if currently streaming
   */
  isActive(): boolean {
    return this.isStreaming;
  }
}

/**
 * Play audio using Web Audio API
 */
export async function playAudioChunk(
  audioData: Float32Array,
  sampleRate: number
): Promise<void> {
  const audioContext = new AudioContext({ sampleRate });
  const audioBuffer = audioContext.createBuffer(1, audioData.length, sampleRate);
  audioBuffer.getChannelData(0).set(audioData);

  const source = audioContext.createBufferSource();
  source.buffer = audioBuffer;
  source.connect(audioContext.destination);
  source.start();

  console.log(`[Audio Playback] Playing ${audioData.length} samples @ ${sampleRate}Hz`);
}
