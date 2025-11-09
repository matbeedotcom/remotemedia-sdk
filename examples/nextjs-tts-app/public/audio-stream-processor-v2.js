// Enhanced AudioWorkletProcessor with better buffering and interpolation
class AudioStreamProcessorV2 extends AudioWorkletProcessor {
  constructor() {
    super();

    // Larger ring buffer for smoother playback
    this.bufferSize = 24000 * 30; // 30 seconds at 24kHz
    this.ringBuffer = new Float32Array(this.bufferSize);
    this.writeIndex = 0;
    this.readIndex = 0;
    this.bufferedSamples = 0;

    // Smoothing parameters
    this.fadeLength = 64; // Samples to fade in/out
    this.lastSample = 0;
    this.isPlaying = false;

    // Dynamic buffer management
    this.chunkTimestamps = []; // Track when chunks arrive
    this.chunkSizes = []; // Track chunk sizes
    this.lastChunkTime = null;
    this.averageChunkInterval = null; // Average time between chunks (ms)
    this.averageChunkSize = null; // Average chunk size (samples)

    // Dynamic thresholds (will be calculated)
    this.minBufferBeforeStart = 9600; // Default 400ms, will adjust
    this.targetBufferSize = 14400; // Default 600ms, will adjust
    this.lowWaterMark = 7200; // Pause playback below this (300ms default)
    this.maxBufferSize = 96000; // Max 4 seconds buffer

    // Buffer health tracking
    this.bufferHealthHistory = []; // Track buffer health over time
    this.lastBufferHealth = 1.0;
    this.underrunHistory = [];
    this.lastUnderrunTime = null;

    // Stats
    this.underruns = 0;
    this.totalSamplesReceived = 0;
    this.totalSamplesPlayed = 0;

    // Handle messages from main thread
    this.port.onmessage = (event) => {
      if (event.data.type === 'audio') {
        this.addAudioData(event.data.samples);
      } else if (event.data.type === 'clear') {
        this.clear();
      } else if (event.data.type === 'status') {
        this.reportStatus();
      }
    };

    // Report status periodically
    this.statusCounter = 0;
    this.statusInterval = 1000; // Report every ~1000 process calls
  }

  addAudioData(samples) {
    const samplesToWrite = samples.length;
    const now = Date.now();
    this.totalSamplesReceived += samplesToWrite;

    // Track chunk timing for dynamic buffer calculation
    if (this.lastChunkTime !== null) {
      const interval = now - this.lastChunkTime;
      this.chunkTimestamps.push(interval);

      // Keep last 20 chunks for moving average
      if (this.chunkTimestamps.length > 20) {
        this.chunkTimestamps.shift();
      }

      // Calculate average chunk interval
      this.averageChunkInterval = this.chunkTimestamps.reduce((a, b) => a + b, 0) / this.chunkTimestamps.length;
    }
    this.lastChunkTime = now;

    // Track chunk sizes
    this.chunkSizes.push(samplesToWrite);
    if (this.chunkSizes.length > 20) {
      this.chunkSizes.shift();
    }
    this.averageChunkSize = this.chunkSizes.reduce((a, b) => a + b, 0) / this.chunkSizes.length;

    // Dynamically adjust buffer thresholds based on streaming rate
    if (this.averageChunkInterval && this.averageChunkSize) {
      // Calculate how many chunks we need to buffer
      // For 24kHz audio, we consume 24000 samples per second
      const consumptionRate = 24000; // samples per second
      const chunksPerSecond = 1000 / this.averageChunkInterval;
      const productionRate = chunksPerSecond * this.averageChunkSize; // samples per second

      // If production is slower than consumption, we need more buffer
      if (productionRate < consumptionRate) {
        // Buffer enough chunks to cover variability
        const chunksToBuffer = Math.ceil(3 / (this.averageChunkInterval / 1000)); // 3 seconds of chunks
        this.minBufferBeforeStart = Math.min(chunksToBuffer * this.averageChunkSize, this.maxBufferSize / 2);
        this.targetBufferSize = Math.min(this.minBufferBeforeStart * 1.5, this.maxBufferSize * 0.75);
        this.lowWaterMark = this.minBufferBeforeStart * 0.75; // Pause at 75% of min buffer
      } else {
        // Production is fast enough, use moderate buffering
        const chunksToBuffer = Math.ceil(1 / (this.averageChunkInterval / 1000)); // 1 second of chunks
        this.minBufferBeforeStart = Math.max(chunksToBuffer * this.averageChunkSize, 4800); // At least 200ms
        this.targetBufferSize = this.minBufferBeforeStart * 1.5;
        this.lowWaterMark = this.minBufferBeforeStart * 0.5; // Pause at 50% of min buffer
      }
    }

    // Check for buffer overflow
    if (this.bufferedSamples + samplesToWrite > this.bufferSize) {
      // Skip old data if buffer is full
      const samplesToSkip = (this.bufferedSamples + samplesToWrite) - this.bufferSize;
      this.readIndex = (this.readIndex + samplesToSkip) % this.bufferSize;
      this.bufferedSamples -= samplesToSkip;
      console.warn(`Buffer overflow, skipping ${samplesToSkip} samples`);
    }

    // Write to ring buffer
    for (let i = 0; i < samplesToWrite; i++) {
      this.ringBuffer[this.writeIndex] = samples[i];
      this.writeIndex = (this.writeIndex + 1) % this.bufferSize;
    }

    this.bufferedSamples += samplesToWrite;

    // Auto-start playback when we have enough buffer
    if (!this.isPlaying && this.bufferedSamples >= this.minBufferBeforeStart) {
      this.isPlaying = true;
      console.log(`Starting playback with ${this.bufferedSamples} samples buffered (min: ${this.minBufferBeforeStart}, avg interval: ${this.averageChunkInterval?.toFixed(1)}ms)`);
    }
  }

  clear() {
    this.writeIndex = 0;
    this.readIndex = 0;
    this.bufferedSamples = 0;
    this.ringBuffer.fill(0);
    this.lastSample = 0;
    this.isPlaying = false;
    this.underruns = 0;
    this.totalSamplesReceived = 0;
    this.totalSamplesPlayed = 0;

    // Reset dynamic tracking
    this.chunkTimestamps = [];
    this.chunkSizes = [];
    this.lastChunkTime = null;
    this.averageChunkInterval = null;
    this.averageChunkSize = null;
    this.underrunHistory = [];
    this.lastUnderrunTime = null;
    this.bufferHealthHistory = [];
    this.lastBufferHealth = 1.0;

    // Reset to defaults
    this.minBufferBeforeStart = 9600;
    this.targetBufferSize = 14400;
    this.lowWaterMark = 7200;
  }

  reportStatus() {
    this.port.postMessage({
      type: 'status',
      bufferedSamples: this.bufferedSamples,
      bufferSize: this.bufferSize,
      isPlaying: this.isPlaying,
      underruns: this.underruns,
      totalReceived: this.totalSamplesReceived,
      totalPlayed: this.totalSamplesPlayed,
      bufferHealth: this.bufferedSamples / this.targetBufferSize,
      // Dynamic buffer metrics
      averageChunkInterval: this.averageChunkInterval,
      averageChunkSize: this.averageChunkSize,
      minBufferBeforeStart: this.minBufferBeforeStart,
      targetBufferSize: this.targetBufferSize,
      lowWaterMark: this.lowWaterMark
    });
  }

  process(inputs, outputs, parameters) {
    const output = outputs[0];
    if (!output || output.length === 0) return true;

    const channel = output[0];
    const framesToRead = channel.length;

    // Report status periodically
    if (++this.statusCounter >= this.statusInterval) {
      this.statusCounter = 0;
      this.reportStatus();
    }

    // Predictive buffer management - pause before underrun
    if (this.isPlaying && this.bufferedSamples < this.lowWaterMark) {
      // Buffer is critically low - pause playback to let it refill
      this.isPlaying = false;
      console.warn(`Buffer critically low (${this.bufferedSamples} < ${this.lowWaterMark}), pausing playback to refill`);
    }

    // If not playing yet, output silence
    if (!this.isPlaying) {
      channel.fill(0);
      return true;
    }

    // Track buffer health for trend analysis
    const currentHealth = this.bufferedSamples / this.targetBufferSize;
    this.bufferHealthHistory.push(currentHealth);
    if (this.bufferHealthHistory.length > 100) {
      this.bufferHealthHistory.shift();
    }

    // Detect rapid buffer drain (health dropping > 20% in last few process calls)
    if (this.bufferHealthHistory.length > 10) {
      const recentHealth = this.bufferHealthHistory.slice(-10);
      const healthTrend = recentHealth[recentHealth.length - 1] - recentHealth[0];

      if (healthTrend < -0.2 && currentHealth < 0.5) {
        // Buffer draining rapidly and already below 50% - pause now
        this.isPlaying = false;
        console.warn(`Rapid buffer drain detected (${(healthTrend * 100).toFixed(1)}% drop), pausing at ${(currentHealth * 100).toFixed(1)}% health`);
        channel.fill(0);
        return true;
      }
    }

    this.lastBufferHealth = currentHealth;

    // Read from ring buffer
    for (let i = 0; i < framesToRead; i++) {
      if (this.bufferedSamples > 0) {
        // Read sample from buffer directly (no smoothing to preserve audio quality)
        const sample = this.ringBuffer[this.readIndex];
        channel[i] = sample;
        this.lastSample = sample;

        this.readIndex = (this.readIndex + 1) % this.bufferSize;
        this.bufferedSamples--;
        this.totalSamplesPlayed++;
      } else {
        // Buffer underrun - we ran out of data
        if (this.isPlaying) {
          this.underruns++;
          this.underrunHistory.push(Date.now());
          this.lastUnderrunTime = Date.now();

          // Keep last 10 underruns for analysis
          if (this.underrunHistory.length > 10) {
            this.underrunHistory.shift();
          }

          // If we're getting frequent underruns, increase buffer requirements
          const recentUnderruns = this.underrunHistory.filter(t => Date.now() - t < 10000).length; // Last 10 seconds
          if (recentUnderruns > 2) {
            // Increase buffer requirements by 50%
            this.minBufferBeforeStart = Math.min(this.minBufferBeforeStart * 1.5, this.maxBufferSize / 2);
            this.targetBufferSize = Math.min(this.targetBufferSize * 1.5, this.maxBufferSize * 0.75);
            console.warn(`Frequent underruns detected, increasing buffer to ${this.minBufferBeforeStart} samples`);
          }

          // Fade out to avoid click
          const fadePosition = Math.min(i, this.fadeLength);
          const fadeGain = 1.0 - (fadePosition / this.fadeLength);
          channel[i] = this.lastSample * fadeGain;

          if (fadePosition < this.fadeLength) {
            this.lastSample *= 0.9; // Gradual fade
          } else {
            this.lastSample = 0;
            this.isPlaying = false; // Stop until more data arrives
            console.log(`Buffer underrun #${this.underruns}, stopping playback. Will restart at ${this.minBufferBeforeStart} samples.`);
          }
        } else {
          // Not playing, output silence
          channel[i] = 0;
        }
      }
    }

    return true; // Keep processor alive
  }
}

registerProcessor('audio-stream-processor-v2', AudioStreamProcessorV2);