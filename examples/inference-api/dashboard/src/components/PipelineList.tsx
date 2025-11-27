import { useEffect, useState } from 'react';
import { listPipelines, PipelineInfo } from '../api/client';
import { Box, Play, Zap } from 'lucide-react';

interface PipelineListProps {
  onSelect?: (pipeline: PipelineInfo) => void;
  selectedPipeline?: string;
}

export function PipelineList({ onSelect, selectedPipeline }: PipelineListProps) {
  const [pipelines, setPipelines] = useState<PipelineInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadPipelines();
  }, []);

  const loadPipelines = async () => {
    try {
      setLoading(true);
      setError(null);
      const response = await listPipelines();
      setPipelines(response.pipelines);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load pipelines');
    } finally {
      setLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="p-4 text-gray-400">
        Loading pipelines...
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-4 text-red-400">
        {error}
        <button onClick={loadPipelines} className="ml-2 underline">
          Retry
        </button>
      </div>
    );
  }

  if (pipelines.length === 0) {
    return (
      <div className="p-4 text-gray-400">
        No pipelines available
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {pipelines.map((pipeline) => (
        <div
          key={pipeline.name}
          onClick={() => onSelect?.(pipeline)}
          className={`
            p-4 rounded-lg border cursor-pointer
            transition-colors duration-150
            ${selectedPipeline === pipeline.name
              ? 'border-blue-500 bg-blue-500/10'
              : 'border-gray-700 hover:border-gray-600 bg-gray-800'
            }
          `}
        >
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-2">
              <Box className="w-5 h-5 text-blue-400" />
              <h3 className="font-medium">{pipeline.name}</h3>
            </div>
            <div className="flex items-center gap-1">
              {pipeline.streaming && (
                <Zap className="w-4 h-4 text-yellow-400" title="Streaming" />
              )}
            </div>
          </div>

          <p className="text-sm text-gray-400 mt-1">
            {pipeline.description || 'No description'}
          </p>

          <div className="flex items-center gap-4 mt-2 text-xs text-gray-500">
            <span>{pipeline.input_type} → {pipeline.output_type}</span>
            <span>v{pipeline.version}</span>
          </div>
        </div>
      ))}
    </div>
  );
}
