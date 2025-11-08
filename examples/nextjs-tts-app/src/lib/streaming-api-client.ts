/**
 * TTS Streaming API Client
 *
 * Simple client that fetches audio from the Next.js API route
 * and streams it to an HTML Audio element for playback.
 */

export interface TTSStreamOptions {
  text: string;
  language?: string;
  voice?: string;
  speed?: number;
}

export interface StreamCallbacks {
  onStart?: () => void;
  onProgress?: (bytesReceived: number) => void;
  onComplete?: () => void;
  onError?: (error: Error) => void;
}

/**
 * Stream TTS audio from the API and return a playable URL
 */
export async function streamTTS(
  options: TTSStreamOptions,
  callbacks?: StreamCallbacks
): Promise<string> {
  try {
    callbacks?.onStart?.();

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

    // Read the stream and collect chunks
    const reader = response.body.getReader();
    const chunks: Uint8Array[] = [];
    let bytesReceived = 0;

    while (true) {
      const { done, value } = await reader.read();

      if (done) {
        break;
      }

      chunks.push(value);
      bytesReceived += value.length;
      callbacks?.onProgress?.(bytesReceived);
    }

    // Combine all chunks into a single buffer
    const totalLength = chunks.reduce((acc, chunk) => acc + chunk.length, 0);
    const audioData = new Uint8Array(totalLength);
    let offset = 0;
    for (const chunk of chunks) {
      audioData.set(chunk, offset);
      offset += chunk.length;
    }

    console.log(`[StreamingAPIClient] Received ${bytesReceived} bytes of audio`);

    // Convert Float32 PCM to WAV format for browser playback
    const sampleRate = 24000;
    const numChannels = 1;
    const wavBuffer = createWavFile(audioData, sampleRate, numChannels);

    // Create a blob URL for the audio
    const blob = new Blob([wavBuffer], { type: 'audio/wav' });
    const url = URL.createObjectURL(blob);

    callbacks?.onComplete?.();

    return url;
  } catch (error) {
    console.error('[StreamingAPIClient] Error:', error);
    callbacks?.onError?.(error as Error);
    throw error;
  }
}

/**
 * Convert raw Float32 PCM to WAV file format
 */
function createWavFile(
  float32Data: Uint8Array,
  sampleRate: number,
  numChannels: number
): ArrayBuffer {
  // Convert Uint8Array to Float32Array
  const float32Array = new Float32Array(
    float32Data.buffer,
    float32Data.byteOffset,
    float32Data.byteLength / 4
  );

  // Convert Float32 to Int16 for WAV
  const int16Data = new Int16Array(float32Array.length);
  for (let i = 0; i < float32Array.length; i++) {
    const sample = Math.max(-1, Math.min(1, float32Array[i]));
    int16Data[i] = sample < 0 ? sample * 0x8000 : sample * 0x7FFF;
  }

  // Create WAV file
  const dataLength = int16Data.length * 2; // 2 bytes per sample
  const buffer = new ArrayBuffer(44 + dataLength);
  const view = new DataView(buffer);

  // WAV header
  writeString(view, 0, 'RIFF');
  view.setUint32(4, 36 + dataLength, true);
  writeString(view, 8, 'WAVE');
  writeString(view, 12, 'fmt ');
  view.setUint32(16, 16, true); // fmt chunk size
  view.setUint16(20, 1, true); // PCM format
  view.setUint16(22, numChannels, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * numChannels * 2, true); // byte rate
  view.setUint16(32, numChannels * 2, true); // block align
  view.setUint16(34, 16, true); // bits per sample
  writeString(view, 36, 'data');
  view.setUint32(40, dataLength, true);

  // Write audio data
  const int16View = new Int16Array(buffer, 44);
  int16View.set(int16Data);

  return buffer;
}

function writeString(view: DataView, offset: number, string: string) {
  for (let i = 0; i < string.length; i++) {
    view.setUint8(offset + i, string.charCodeAt(i));
  }
}

/**
 * Cancel a streaming request (if using AbortController)
 */
export function cancelStream(abortController: AbortController) {
  abortController.abort();
}
