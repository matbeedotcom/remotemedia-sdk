import { PredictResponse } from '../api/client';
import { Clock, FileType, Code } from 'lucide-react';

interface ResponseViewerProps {
  response: PredictResponse | null;
}

export function ResponseViewer({ response }: ResponseViewerProps) {
  if (!response) {
    return (
      <div className="p-4 text-gray-400 text-center">
        No response yet. Make a prediction to see results.
      </div>
    );
  }

  // Decode output
  let decodedOutput: string = '';
  if (response.output_data) {
    try {
      decodedOutput = atob(response.output_data);
      // If it's text, show directly; if binary, show info
      if (response.output_type !== 'text') {
        decodedOutput = `[Binary data: ${response.output_data.length} bytes]`;
      }
    } catch {
      decodedOutput = response.output_data;
    }
  }

  return (
    <div className="space-y-4">
      {/* Metadata */}
      <div className="flex items-center gap-4 text-sm text-gray-400">
        <div className="flex items-center gap-1">
          <FileType className="w-4 h-4" />
          <span>{response.output_type}</span>
        </div>
        <div className="flex items-center gap-1">
          <Clock className="w-4 h-4" />
          <span>{response.timing_ms.toFixed(2)} ms</span>
        </div>
      </div>

      {/* Output */}
      <div className="border border-gray-700 rounded-lg">
        <div className="px-4 py-2 border-b border-gray-700 flex items-center gap-2">
          <Code className="w-4 h-4" />
          <span className="text-sm font-medium">Output</span>
        </div>
        <div className="p-4">
          {response.output_type === 'audio' ? (
            <div className="space-y-2">
              <p className="text-gray-400">Audio output</p>
              {response.output_data && (
                <audio
                  controls
                  src={`data:audio/wav;base64,${response.output_data}`}
                  className="w-full"
                />
              )}
            </div>
          ) : (
            <pre className="whitespace-pre-wrap text-sm bg-gray-800 p-4 rounded">
              {decodedOutput || '(empty)'}
            </pre>
          )}
        </div>
      </div>

      {/* Metadata */}
      {Object.keys(response.metadata).length > 0 && (
        <div className="border border-gray-700 rounded-lg">
          <div className="px-4 py-2 border-b border-gray-700">
            <span className="text-sm font-medium">Metadata</span>
          </div>
          <pre className="p-4 text-sm bg-gray-800 overflow-x-auto">
            {JSON.stringify(response.metadata, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}
