import { useSessionStore } from '@/store/session';
import clsx from 'clsx';

export function HealthScore() {
  const healthScore = useSessionStore((s) => s.healthScore);
  const healthStatus = useSessionStore((s) => s.healthStatus);
  const status = useSessionStore((s) => s.status);

  // Don't show if not streaming
  if (status !== 'streaming' || healthScore === null) {
    return null;
  }

  const percentage = Math.round(healthScore * 100);

  return (
    <div className="p-6 border-b border-surface-elevated">
      <div
        className={clsx(
          'text-5xl font-bold text-center transition-colors',
          healthStatus === 'ok' && 'text-status-ok',
          healthStatus === 'degraded' && 'text-status-warning',
          healthStatus === 'unhealthy' && 'text-status-error'
        )}
      >
        {percentage}%
      </div>
      <p className="text-center text-sm text-text-muted mt-1">Health Score</p>
    </div>
  );
}
