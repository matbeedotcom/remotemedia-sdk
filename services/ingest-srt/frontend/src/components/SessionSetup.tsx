import { useSessionStore } from '@/store/session';
import { PIPELINES, type PipelineTemplate } from '@/types/session';
import { PipelineCard } from './PipelineCard';

export function SessionSetup() {
  const selectedPipeline = useSessionStore((s) => s.selectedPipeline);
  const selectPipeline = useSessionStore((s) => s.selectPipeline);

  const handlePipelineSelect = (pipeline: PipelineTemplate) => {
    selectPipeline(pipeline);
  };

  // Group pipelines by category
  const categories = ['business', 'technical', 'audio', 'video'] as const;

  return (
    <div className="space-y-5">
      {/* Pipeline cards in a contained area */}
      <div className="bg-surface-secondary/30 rounded-xl p-4 space-y-3">
        {categories.map((category) => {
          const categoryPipelines = PIPELINES.filter((p) => p.category === category);
          if (categoryPipelines.length === 0) return null;

          return (
            <div key={category}>
              <p className="text-xs text-text-muted uppercase tracking-wider mb-1.5">
                {category}
              </p>
              <div className="space-y-1.5">
                {categoryPipelines.map((pipeline) => (
                  <PipelineCard
                    key={pipeline.id}
                    pipeline={pipeline}
                    isSelected={selectedPipeline?.id === pipeline.id}
                    onSelect={handlePipelineSelect}
                  />
                ))}
              </div>
            </div>
          );
        })}
      </div>

      {/* Hint about what comes next + reassurance */}
      <div className="text-center space-y-1">
        <p className="text-xs text-text-muted">
          Once selected, you'll get a command to start observing.
        </p>
        <p className="text-xs text-text-muted/70">
          You can change this later.
        </p>
      </div>
    </div>
  );
}
