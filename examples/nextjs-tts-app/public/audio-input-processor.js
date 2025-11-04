/**
 * AudioWorklet processor for capturing microphone input
 * Replaces the deprecated ScriptProcessorNode
 *
 * Sends chunks of ~256ms duration regardless of sample rate
 * to ensure consistent processing on the backend
 */

class AudioInputProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();

    // Calculate buffer size for ~64ms of audio at the current sample rate
    // At 16kHz: 1024 samples = 64ms (divides perfectly into 512-sample VAD chunks)
    // At 48kHz: 3072 samples = 64ms
    const sampleRate = options.processorOptions?.sampleRate || 48000;
    this.bufferSize = Math.floor(sampleRate * 0.064); // 64ms

    console.log(`[AudioInputProcessor] Initialized with buffer size: ${this.bufferSize} samples @ ${sampleRate}Hz (${(this.bufferSize / sampleRate * 1000).toFixed(1)}ms)`);

    this.buffer = new Float32Array(this.bufferSize);
    this.bufferIndex = 0;
  }

  process(inputs, outputs, parameters) {
    const input = inputs[0];

    if (!input || input.length === 0) {
      return true;
    }

    const inputChannel = input[0]; // Mono input

    // Accumulate samples
    for (let i = 0; i < inputChannel.length; i++) {
      this.buffer[this.bufferIndex++] = inputChannel[i];

      // When buffer is full, send it to main thread
      if (this.bufferIndex >= this.bufferSize) {
        // Send chunk to main thread
        this.port.postMessage({
          type: 'audiodata',
          samples: this.buffer.slice(0)
        });

        // Reset buffer
        this.bufferIndex = 0;
      }
    }

    return true; // Keep processor alive
  }
}

registerProcessor('audio-input-processor', AudioInputProcessor);
