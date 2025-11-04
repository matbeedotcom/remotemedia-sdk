/**
 * Test script to verify resource-loader.ts works correctly
 */

const path = require('path');

async function testResourceLoader() {
  console.log('Testing resource loader...\n');

  // Import the TypeScript module
  const { requireFile } = await import('./src/lib/resource-loader.ts');

  // Test loading the demo WAV file
  const audioPath = 'C:/Users/mail/dev/personal/remotemedia-sdk/examples/transcribe_demo.wav';

  console.log(`Loading audio file: ${audioPath}`);

  try {
    const result = await requireFile(audioPath);

    if (result.audio) {
      console.log('\n✅ Successfully loaded audio!');
      console.log(`   - Sample Rate: ${result.audio.sampleRate} Hz`);
      console.log(`   - Channels: ${result.audio.channels}`);
      console.log(`   - Format: ${result.audio.format === 1 ? 'F32' : 'I16'}`);
      console.log(`   - Num Samples: ${result.audio.numSamples}`);
      console.log(`   - Duration: ${(result.audio.numSamples / result.audio.sampleRate).toFixed(2)}s`);
      console.log(`   - Buffer Size: ${result.audio.samples.length} bytes`);

      // Verify the buffer is actually Float32 data
      const float32View = new Float32Array(
        result.audio.samples.buffer,
        result.audio.samples.byteOffset,
        result.audio.samples.byteLength / 4
      );
      console.log(`   - First 5 samples: [${Array.from(float32View.slice(0, 5)).map(v => v.toFixed(4)).join(', ')}]`);

      return true;
    } else {
      console.error('❌ No audio data in result');
      return false;
    }
  } catch (error) {
    console.error('❌ Error loading audio:', error);
    return false;
  }
}

// Run test
testResourceLoader()
  .then(success => {
    console.log(`\nTest ${success ? 'PASSED' : 'FAILED'}`);
    process.exit(success ? 0 : 1);
  })
  .catch(error => {
    console.error('Test error:', error);
    process.exit(1);
  });
