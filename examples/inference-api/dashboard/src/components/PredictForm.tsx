import { useState, useRef } from 'react';
import { predict, PredictResponse, PipelineInfo } from '../api/client';
import { Upload, Send, Loader2 } from 'lucide-react';

interface PredictFormProps {
  pipeline: PipelineInfo | null;
  onResult?: (result: PredictResponse) => void;
}

export function PredictForm({ pipeline, onResult }: PredictFormProps) {
  const [inputText, setInputText] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!pipeline) return;

    setLoading(true);
    setError(null);

    try {
      let inputData: string | null = null;
      let inputType = 'text';

      if (file) {
        // Read file as base64
        const buffer = await file.arrayBuffer();
        inputData = btoa(String.fromCharCode(...new Uint8Array(buffer)));
        inputType = file.type.startsWith('audio/') ? 'audio' : 'text';
      } else if (inputText) {
        inputData = btoa(inputText);
        inputType = 'text';
      }

      const result = await predict({
        pipeline: pipeline.name,
        input_data: inputData,
        input_type: inputType,
      });

      onResult?.(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Prediction failed');
    } finally {
      setLoading(false);
    }
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const selectedFile = e.target.files?.[0];
    if (selectedFile) {
      setFile(selectedFile);
      setInputText('');
    }
  };

  const handleTextChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInputText(e.target.value);
    setFile(null);
  };

  if (!pipeline) {
    return (
      <div className="p-4 text-gray-400 text-center">
        Select a pipeline to make predictions
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div>
        <label className="block text-sm font-medium mb-2">
          Input ({pipeline.input_type})
        </label>

        {pipeline.input_type === 'audio' ? (
          <div
            onClick={() => fileInputRef.current?.click()}
            className="border-2 border-dashed border-gray-600 rounded-lg p-8
                     hover:border-gray-500 cursor-pointer transition-colors
                     flex flex-col items-center gap-2"
          >
            <Upload className="w-8 h-8 text-gray-400" />
            <span className="text-gray-400">
              {file ? file.name : 'Click to upload audio file'}
            </span>
            <input
              ref={fileInputRef}
              type="file"
              accept="audio/*"
              onChange={handleFileChange}
              className="hidden"
            />
          </div>
        ) : (
          <textarea
            value={inputText}
            onChange={handleTextChange}
            placeholder="Enter text input..."
            rows={4}
            className="w-full px-3 py-2 bg-gray-800 border border-gray-600 rounded-lg
                     focus:outline-none focus:border-blue-500 resize-none"
          />
        )}
      </div>

      {error && (
        <div className="text-red-400 text-sm">
          {error}
        </div>
      )}

      <button
        type="submit"
        disabled={loading || (!file && !inputText)}
        className="w-full flex items-center justify-center gap-2 px-4 py-2
                 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-600
                 rounded-lg transition-colors"
      >
        {loading ? (
          <>
            <Loader2 className="w-4 h-4 animate-spin" />
            Processing...
          </>
        ) : (
          <>
            <Send className="w-4 h-4" />
            Predict
          </>
        )}
      </button>
    </form>
  );
}
