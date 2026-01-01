import { useSessionStore } from '@/store/session';
import clsx from 'clsx';

export function StatusHeader() {
  const status = useSessionStore((s) => s.status);
  const healthStatus = useSessionStore((s) => s.healthStatus);
  const activeContributors = useSessionStore((s) => s.activeContributors);

  const isLive = status === 'streaming';

  // Determine display status
  const displayStatus = isLive
    ? healthStatus === 'unhealthy'
      ? 'Session Unhealthy'
      : healthStatus === 'degraded'
        ? 'Session Degraded'
        : 'No active issues'
    : status === 'disconnected'
      ? 'Disconnected'
      : status === 'connecting'
        ? 'Connecting...'
        : status === 'created'
          ? 'Waiting for stream...'
          : null;

  if (status === 'idle') {
    return null;
  }

  return (
    <div
      className={clsx(
        'fixed top-0 left-0 right-0 z-50 px-6 py-3 transition-all duration-300',
        'flex items-center justify-between',
        'backdrop-blur-sm border-b',
        isLive && healthStatus === 'ok' && 'bg-surface-secondary/90 border-status-ok/20',
        isLive && healthStatus === 'degraded' && 'bg-surface-secondary/90 border-status-warning/30',
        isLive && healthStatus === 'unhealthy' && 'bg-surface-secondary/90 border-status-error/30',
        !isLive && 'bg-surface-secondary/80 border-text-muted/20'
      )}
    >
      <div className="flex items-center gap-4">
        {/* Live indicator */}
        {isLive && (
          <div className="flex items-center gap-2">
            <div
              className={clsx(
                'w-2 h-2 rounded-full animate-pulse-subtle',
                healthStatus === 'ok' && 'bg-status-ok',
                healthStatus === 'degraded' && 'bg-status-warning',
                healthStatus === 'unhealthy' && 'bg-status-error'
              )}
            />
            <span className="text-sm font-medium text-text-primary">LIVE</span>
          </div>
        )}

        {/* Status text */}
        <span
          className={clsx(
            'text-sm',
            isLive && healthStatus === 'ok' && 'text-status-ok',
            isLive && healthStatus === 'degraded' && 'text-status-warning',
            isLive && healthStatus === 'unhealthy' && 'text-status-error',
            !isLive && 'text-text-secondary'
          )}
        >
          {displayStatus}
        </span>
      </div>

      {/* Active contributors */}
      {isLive && activeContributors.length > 0 && (
        <div className="flex items-center gap-2 text-xs text-text-muted">
          <span>Contributors:</span>
          {activeContributors.slice(0, 3).map((c) => (
            <span
              key={c}
              className="px-2 py-0.5 rounded bg-surface-elevated text-status-warning"
            >
              {c}
            </span>
          ))}
          {activeContributors.length > 3 && (
            <span className="text-text-muted">+{activeContributors.length - 3}</span>
          )}
        </div>
      )}
    </div>
  );
}
