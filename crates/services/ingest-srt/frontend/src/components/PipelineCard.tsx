import clsx from 'clsx';
import type { PipelineTemplate } from '@/types/session';

interface PipelineCardProps {
  pipeline: PipelineTemplate;
  isSelected: boolean;
  onSelect: (pipeline: PipelineTemplate) => void;
}

export function PipelineCard({ pipeline, isSelected, onSelect }: PipelineCardProps) {
  return (
    <button
      type="button"
      onClick={() => onSelect(pipeline)}
      className={clsx(
        'w-full text-left px-3 py-2.5 rounded-lg border transition-all',
        'hover:border-status-info/50 hover:bg-surface-elevated/50',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-status-info/50',
        isSelected
          ? 'border-status-info bg-surface-elevated'
          : 'border-surface-elevated/50 bg-surface-card/50'
      )}
    >
      <h3 className={clsx(
        'font-medium text-sm',
        isSelected ? 'text-status-info' : 'text-text-primary'
      )}>
        {pipeline.name}
      </h3>
      <p className="text-xs text-text-muted mt-0.5">
        {pipeline.description}
      </p>
    </button>
  );
}
