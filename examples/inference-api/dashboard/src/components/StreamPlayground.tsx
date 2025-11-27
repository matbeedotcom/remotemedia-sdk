import { useState, useRef, useCallback } from 'react';
import {
  startStream,
  sendStreamInput,
  closeStream,
  streamOutput,
  PipelineInfo,
} from '../api/client';
import { Play, Square, Mic, MicOff, Loader2 } from 'lucide-react';

interface StreamOutput {
  output_type: string;
  output_data: string | null;
  metadata: Record<string, unknown>;
  timestamp_ms: number;
}

interface StreamPlaygroundProps {
  pipeline: PipelineInfo | null;
}

export function StreamPlayground({ pipeline }: StreamPlaygroundProps) {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [outputs, setOutputs] = useState<StreamOutput[]>([]);
  const [isRecording, setIsRecording] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const eventSourceRef = useRef<EventSource | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);

  const handleStart = async () => {
    if (!pipeline) return;

    setLoading(true);
    setError(null);
    setOutputs([]);

    try {
      const response = await startStream(pipeline.name);
      setSessionId(response.session_id);

      // Connect to SSE stream
      const source = streamOutput(
        response.session_id,
        (event) => {
          const data = JSON.parse(event.data) as StreamOutput;
          setOutputs((prev) => [...prev, data]);
        },
        () => {
          setError('Connection lost');
        }
      );

      eventSourceRef.current = source;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to start stream');
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    if (isRecording) {
      mediaRecorderRef.current?.stop();
      setIsRecording(false);
    }

    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }

    if (sessionId) {
      try {
        await closeStream(sessionId);
      } catch (err) {
        console.error('Failed to close stream:', err);
      }
      setSessionId(null);
    }
  };

  const handleRecordToggle = useCallback(async () => {
    if (!sessionId) return;

    if (isRecording) {
      mediaRecorderRef.current?.stop();
      setIsRecording(false);
      return;
    }

    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const recorder = new MediaRecorder(stream);

      recorder.ondataavailable = async (event) => {
        if (event.data.size > 0) {
          const buffer = await event.data.arrayBuffer();
          const base64 = btoa(String.fromCharCode(...new Uint8Array(buffer)));
          await sendStreamInput(sessionId, base64, 'audio');
        }
      };

      recorder.start(1000); // Send chunks every second
      mediaRecorderRef.current = recorder;
      setIsRecording(true);
    } catch (err) {
      setError('Failed to access microphone');
    }
  }, [sessionId, isRecording]);

  if (!pipeline) {
    return (
      <div className="p-4 text-gray-400 text-center">
        Select a streaming pipeline to use the playground
      </div>
    );
  }

  if (!pipeline.streaming) {
    return (
      <div className="p-4 text-gray-400 text-center">
        This pipeline does not support streaming
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Controls */}
      <div className="flex items-center gap-2">
        {!sessionId ? (
          <button
            onClick={handleStart}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 bg-green-600
                     hover:bg-green-700 rounded-lg transition-colors"
          >
            {loading ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Play className="w-4 h-4" />
            )}
            Start Session
          </button>
        ) : (
          <>
            <button
              onClick={handleStop}
              className="flex items-center gap-2 px-4 py-2 bg-red-600
                       hover:bg-red-700 rounded-lg transition-colors"
            >
              <Square className="w-4 h-4" />
              Stop
            </button>

            <button
              onClick={handleRecordToggle}
              className={`flex items-center gap-2 px-4 py-2 rounded-lg
                        transition-colors ${
                          isRecording
                            ? 'bg-red-500 hover:bg-red-600 animate-pulse'
                            : 'bg-blue-600 hover:bg-blue-700'
                        }`}
            >
              {isRecording ? (
                <>
                  <MicOff className="w-4 h-4" />
                  Stop Recording
                </>
              ) : (
                <>
                  <Mic className="w-4 h-4" />
                  Record
                </>
              )}
            </button>
          </>
        )}
      </div>

      {error && (
        <div className="text-red-400 text-sm">{error}</div>
      )}

      {/* Output stream */}
      <div className="border border-gray-700 rounded-lg p-4 min-h-[200px] max-h-[400px] overflow-y-auto">
        {outputs.length === 0 ? (
          <p className="text-gray-400 text-center">
            {sessionId ? 'Waiting for outputs...' : 'Start a session to see outputs'}
          </p>
        ) : (
          <div className="space-y-2">
            {outputs.map((output, i) => (
              <div key={i} className="p-2 bg-gray-800 rounded">
                <div className="flex justify-between text-xs text-gray-400 mb-1">
                  <span>{output.output_type}</span>
                  <span>{new Date(output.timestamp_ms).toLocaleTimeString()}</span>
                </div>
                <div className="text-sm">
                  {output.output_data
                    ? atob(output.output_data)
                    : JSON.stringify(output.metadata)}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
