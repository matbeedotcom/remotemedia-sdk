import { useState } from 'preact/hooks';
import { executePipeline } from '../lib/api';
import { ResultDisplay } from './ResultDisplay';
import { AudioRecorder } from './AudioRecorder';

type InputType = 'text' | 'json' | 'audio';

export function GenericPanel() {
  const [inputType, setInputType] = useState<InputType>('text');
  const [inputValue, setInputValue] = useState('');
  const [result, setResult] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [audioBlob, setAudioBlob] = useState<Blob | null>(null);

  const handleExecute = async () => {
    setError(null);
    setResult(null);
    setLoading(true);

    try {
      let input: any;
      if (inputType === 'json') {
        try {
          input = JSON.parse(inputValue);
        } catch {
          throw new Error('Invalid JSON input');
        }
      } else if (inputType === 'audio') {
        if (!audioBlob) {
          throw new Error('No audio recorded');
        }
        // Convert blob to base64
        const buffer = await audioBlob.arrayBuffer();
        const base64 = btoa(
          new Uint8Array(buffer).reduce((data, byte) => data + String.fromCharCode(byte), '')
        );
        input = { Audio: { data: base64, format: 'webm' } };
      } else {
        input = { Text: inputValue };
      }

      const res = await executePipeline(input);
      setResult(res);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div class="panel">
      <h2>Pipeline Execution</h2>

      <div class="input-type-row">
        <label style={{ marginBottom: 0 }}>Input type:</label>
        <select
          value={inputType}
          onChange={(e) => setInputType((e.target as HTMLSelectElement).value as InputType)}
          style={{ width: 'auto' }}
        >
          <option value="text">Text</option>
          <option value="json">JSON</option>
          <option value="audio">Audio (record)</option>
        </select>
      </div>

      {inputType === 'audio' ? (
        <div>
          <AudioRecorder onRecorded={setAudioBlob} />
          {audioBlob && (
            <div class="audio-visualizer">
              Audio recorded ({(audioBlob.size / 1024).toFixed(1)} KB)
            </div>
          )}
        </div>
      ) : (
        <textarea
          placeholder={inputType === 'json' ? '{"key": "value"}' : 'Enter text input...'}
          value={inputValue}
          onInput={(e) => setInputValue((e.target as HTMLTextAreaElement).value)}
        />
      )}

      <div class="btn-group">
        <button
          class="btn btn-primary"
          onClick={handleExecute}
          disabled={loading}
        >
          {loading ? 'Executing...' : 'Execute'}
        </button>
      </div>

      {error && <div class="error">{error}</div>}

      {loading && <div class="loading">Processing pipeline...</div>}

      {result && (
        <div style={{ marginTop: '1rem' }}>
          <label>Result</label>
          <ResultDisplay result={result} />
        </div>
      )}
    </div>
  );
}
