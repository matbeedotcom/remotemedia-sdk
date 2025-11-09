// AudioWorkletProcessor for seamless streaming audio playback
class AudioStreamProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    // Ring buffer for audio data (24kHz audio from LFM2)
    this.bufferSize = 24000 * 10; // 10 seconds at 24kHz
    this.ringBuffer = new Float32Array(this.bufferSize);
    this.writeIndex = 0;
    this.readIndex = 0;
    this.bufferedSamples = 0;

    // Note: AudioContext might run at different rate (e.g., 48kHz)
    // but we're storing 24kHz samples, so no resampling needed
    // The AudioContext will handle the rate conversion

    // Handle messages from main thread
    this.port.onmessage = (event) => {
      if (event.data.type === 'audio') {
        this.addAudioData(event.data.samples);
      } else if (event.data.type === 'clear') {
        this.clear();
      }
    };
  }

  addAudioData(samples) {
    const samplesToWrite = samples.length;

    // Write to ring buffer
    for (let i = 0; i < samplesToWrite; i++) {
      this.ringBuffer[this.writeIndex] = samples[i];
      this.writeIndex = (this.writeIndex + 1) % this.bufferSize;
    }

    this.bufferedSamples = Math.min(this.bufferedSamples + samplesToWrite, this.bufferSize);

    // Report buffer status
    this.port.postMessage({
      type: 'bufferStatus',
      bufferedSamples: this.bufferedSamples,
      bufferSize: this.bufferSize
    });
  }

  clear() {
    this.writeIndex = 0;
    this.readIndex = 0;
    this.bufferedSamples = 0;
    this.ringBuffer.fill(0);
  }

  process(inputs, outputs, parameters) {
    const output = outputs[0];
    if (!output || output.length === 0) return true;

    const channel = output[0];
    const framesToRead = channel.length;

    // Read from ring buffer
    for (let i = 0; i < framesToRead; i++) {
      if (this.bufferedSamples > 0) {
        // Read sample from buffer
        channel[i] = this.ringBuffer[this.readIndex];
        this.readIndex = (this.readIndex + 1) % this.bufferSize;
        this.bufferedSamples--;
      } else {
        // No data available, output silence
        channel[i] = 0;
      }
    }

    return true; // Keep processor alive
  }
}

registerProcessor('audio-stream-processor', AudioStreamProcessor);