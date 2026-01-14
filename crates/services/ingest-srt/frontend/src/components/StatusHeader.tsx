import { useSessionStore } from '@/store/session';
import { useDemoMode } from '@/hooks/usePersona';
import clsx from 'clsx';

export function StatusHeader() {
  const status = useSessionStore((s) => s.status);
  const healthStatus = useSessionStore((s) => s.healthStatus);
  const activeContributors = useSessionStore((s) => s.activeContributors);
  const selectedPipeline = useSessionStore((s) => s.selectedPipeline);
  const isDemo = useDemoMode();

  const isLive = status === 'streaming';
  const isReady = (status === 'created' || status === 'connecting') && selectedPipeline;
  const isDisconnected = status === 'disconnected';

  // Determine display status and label
  let statusLabel: string;
  let statusText: string;

  if (isLive) {
    statusLabel = 'LIVE';
    if (healthStatus === 'unhealthy') {
      statusText = 'Session unhealthy';
    } else if (healthStatus === 'degraded') {
      statusText = 'Session degraded';
    } else {
      statusText = 'No active issues';
    }
  } else if (isDisconnected) {
    statusLabel = 'DISCONNECTED';
    statusText = 'Session complete';
  } else if (isReady) {
    statusLabel = 'READY';
    statusText = 'Waiting for media';
  } else {
    // Idle state - don't show header
    return null;
  }

  return (
    <div
      className={clsx(
        'px-6 py-3 transition-all duration-150',
        'flex items-center justify-between',
        'border-b',
        isLive && healthStatus === 'ok' && 'bg-surface-secondary border-status-ok/20',
        isLive && healthStatus === 'degraded' && 'bg-surface-secondary border-status-warning/30',
        isLive && healthStatus === 'unhealthy' && 'bg-surface-secondary border-status-error/30',
        isReady && 'bg-surface-secondary border-status-info/20',
        isDisconnected && 'bg-surface-secondary border-text-muted/20'
      )}
    >
      <div className="flex items-center gap-3">
        {/* Status indicator */}
        <div className="flex items-center gap-2">
          <div
            className={clsx(
              'w-2 h-2 rounded-full',
              isLive && 'animate-pulse-subtle',
              isLive && healthStatus === 'ok' && 'bg-status-ok',
              isLive && healthStatus === 'degraded' && 'bg-status-warning',
              isLive && healthStatus === 'unhealthy' && 'bg-status-error',
              isReady && 'bg-status-info',
              isDisconnected && 'bg-text-muted'
            )}
          />
          <span
            className={clsx(
              'text-sm font-medium',
              isLive && 'text-text-primary',
              isReady && 'text-status-info',
              isDisconnected && 'text-text-muted'
            )}
          >
            {statusLabel}
          </span>
          {isDemo && (
            <span className="px-2 py-0.5 text-xs font-medium rounded bg-accent-speech/20 text-accent-speech">
              DEMO
            </span>
          )}
        </div>

        {/* Status text */}
        <span className="text-sm text-text-secondary">
          {statusText}
        </span>
      </div>

      {/* Active contributors (only when live and degraded/unhealthy) */}
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
