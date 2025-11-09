/**
 * Real-time TTS Streaming Client
 *
 * Uses Web Audio API to enable audio playback as soon as the first chunk arrives,
 * rather than waiting for the entire audio to download.
 */

export interface TTSStreamOptions {
  text: string;
  language?: string;
  voice?: string;
  speed?: number;
}

export interface StreamCallbacks {
  onStart?: () => void;
  onFirstChunk?: () => void; // Called when first audio chunk is ready for playback
  onChunk?: () => void; // Called every time a chunk is played
  onProgress?: (bytesReceived: number) => void;
  onComplete?: () => void;
  onError?: (error: Error) => void;
}

/**
 * Stream TTS audio with real-time playback using Web Audio API
 */
export async function streamTTSRealtime(
  options: TTSStreamOptions,
  callbacks?: StreamCallbacks
): Promise<HTMLAudioElement> {
  try {
    callbacks?.onStart?.();

    // Create Web Audio API context
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();

    // Track audio chunks and playback timing
    const audioChunks: Float32Array[] = [];
    let nextPlayTime = audioContext.currentTime;
    const sampleRate = 24000;
    let hasStartedPlayback = false;

    // Create a dummy audio element for compatibility with existing code
    const audio = new Audio();
    audio.autoplay = false; // We'll manage playback via Web Audio API

    // Create a silent data URL to prevent 'error' events
    audio.src = 'data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQAAAAA=';

    // Schedule audio chunk for playback
    const playChunk = (float32Array: Float32Array) => {
      const audioBuffer = audioContext.createBuffer(1, float32Array.length, sampleRate);
      audioBuffer.getChannelData(0).set(float32Array);

      const source = audioContext.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(audioContext.destination);

      // Schedule this chunk to play after the previous one
      const startTime = Math.max(nextPlayTime, audioContext.currentTime);
      source.start(startTime);

      // Update next play time
      nextPlayTime = startTime + audioBuffer.duration;

      console.log(`[RealtimeStreaming] Scheduled chunk: ${float32Array.length} samples, start: ${startTime.toFixed(2)}s, duration: ${audioBuffer.duration.toFixed(2)}s`);

      // Call chunk callback
      callbacks?.onChunk?.();

      if (!hasStartedPlayback) {
        hasStartedPlayback = true;
        callbacks?.onFirstChunk?.();
        // Simulate audio element play event
        audio.dispatchEvent(new Event('play'));
      }
    };

    // Make request to streaming API
    const response = await fetch('/api/tts/stream', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        text: options.text,
        language: options.language || 'en-us',
        voice: options.voice || 'af_bella',
        speed: options.speed || 1.0,
      }),
    });

    if (!response.ok) {
      throw new Error(`API error: ${response.status} ${response.statusText}`);
    }

    if (!response.body) {
      throw new Error('No response body');
    }

    // Read the stream and play chunks as they arrive
    const reader = response.body.getReader();
    let bytesReceived = 0;
    let buffer: Uint8Array = new Uint8Array(0);
    const MIN_CHUNK_SIZE = 48000; // ~0.5s of audio (24000 samples/s * 4 bytes/sample * 0.5s)

    const processChunks = async () => {
      try {
        while (true) {
          const { done, value } = await reader.read();

          if (done) {
            // Play any remaining buffer
            if (buffer.length >= 4) { // At least one Float32 sample
              const float32Array = new Float32Array(
                buffer.buffer,
                buffer.byteOffset,
                buffer.byteLength / 4
              );
              playChunk(float32Array);
            }

            console.log(`[RealtimeStreaming] Stream complete: ${bytesReceived} bytes`);

            // Schedule 'ended' event after last chunk finishes
            const endTime = nextPlayTime - audioContext.currentTime;
            setTimeout(() => {
              audio.dispatchEvent(new Event('ended'));
              callbacks?.onComplete?.();
            }, endTime * 1000);

            break;
          }

          // Append to buffer
          const newBuffer = new Uint8Array(buffer.length + value.length);
          newBuffer.set(buffer);
          newBuffer.set(value, buffer.length);
          buffer = newBuffer;

          bytesReceived += value.length;
          callbacks?.onProgress?.(bytesReceived);

          // Play chunks when we have enough data
          while (buffer.length >= MIN_CHUNK_SIZE) {
            const chunkBytes = buffer.slice(0, MIN_CHUNK_SIZE);
            buffer = buffer.slice(MIN_CHUNK_SIZE);

            // Convert to Float32Array and play
            const float32Array = new Float32Array(
              chunkBytes.buffer,
              chunkBytes.byteOffset,
              chunkBytes.byteLength / 4
            );

            playChunk(float32Array);
          }
        }
      } catch (error) {
        console.error('[RealtimeStreaming] Stream processing error:', error);
        callbacks?.onError?.(error as Error);
        throw error;
      }
    };

    // Start processing stream
    processChunks();

    return audio;
  } catch (error) {
    console.error('[RealtimeStreaming] Error:', error);
    callbacks?.onError?.(error as Error);
    throw error;
  }
}

