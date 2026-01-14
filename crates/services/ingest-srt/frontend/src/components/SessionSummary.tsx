import { useMemo } from 'react';
import { useSessionStore } from '@/store/session';
import { getEventCategory } from '@/types/events';
import type { SessionSummary as SessionSummaryType } from '@/types/session';

/** Compute session summary from events */
function useSessionSummary(): SessionSummaryType | null {
  const events = useSessionStore((s) => s.events);
  const startedAt = useSessionStore((s) => s.startedAt);
  const session = useSessionStore((s) => s.session);

  return useMemo(() => {
    if (!startedAt) return null;

    const durationMs = Date.now() - startedAt;

    // Count alerts by type
    const alertCounts: Record<string, number> = {};
    let totalAlerts = 0;

    for (const event of events) {
      const category = getEventCategory(event.event_type);
      if (category === 'alert' || category === 'incident') {
        const type = event.event_type;
        alertCounts[type] = (alertCounts[type] || 0) + 1;
        totalAlerts++;
      }
    }

    return {
      durationMs,
      alertCounts,
      totalAlerts,
      webhookConfigured: !!session?.webhook_url,
    };
  }, [events, startedAt, session]);
}

/** Format duration for display */
function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;

  if (minutes === 0) {
    return `${seconds}s`;
  }
  return `${minutes}m ${seconds}s`;
}

/** Get human-readable label for event type */
function getAlertLabel(eventType: string): string {
  switch (eventType) {
    case 'audio.silence':
      return 'Silence';
    case 'audio.clipping':
      return 'Clipping';
    case 'audio.low_volume':
      return 'Low volume';
    case 'audio.channel_imbalance':
      return 'Channel imbalance';
    case 'timing.jitter_spike':
      return 'Jitter spike';
    case 'timing.clock_drift':
      return 'Clock drift';
    case 'incident.created':
      return 'Incidents';
    default:
      return eventType.replace(/[._]/g, ' ');
  }
}

export function SessionSummary() {
  const summary = useSessionSummary();
  const selectedPipeline = useSessionStore((s) => s.selectedPipeline);
  const reset = useSessionStore((s) => s.reset);
  const selectPipeline = useSessionStore((s) => s.selectPipeline);

  const handleRunAgain = () => {
    // Keep the pipeline selected, but reset the session to ready state
    const pipeline = selectedPipeline;
    reset();
    if (pipeline) {
      selectPipeline(pipeline);
    }
  };

  const handleChangePipeline = () => {
    // Full reset to idle state
    reset();
  };

  if (!summary) {
    return (
      <div className="flex-1 flex items-center justify-center text-text-muted">
        <p>No session data available</p>
      </div>
    );
  }

  const alertEntries = Object.entries(summary.alertCounts).sort((a, b) => b[1] - a[1]);

  return (
    <div className="flex-1 flex flex-col animate-fade-in">
      <div className="px-6 py-4 border-b border-surface-elevated">
        <h2 className="text-lg font-medium text-text-primary">Session Complete</h2>
      </div>

      <div className="flex-1 overflow-y-auto p-6 space-y-6">
        {/* Duration */}
        <div className="text-center py-4">
          <p className="text-sm text-text-muted mb-1">Duration</p>
          <p className="text-3xl font-light text-text-primary">
            {formatDuration(summary.durationMs)}
          </p>
        </div>

        {/* Alert counts */}
        {summary.totalAlerts > 0 ? (
          <div className="space-y-3">
            <h3 className="text-xs font-medium text-text-muted uppercase tracking-wider">
              Alerts detected
            </h3>
            <div className="bg-surface-card rounded-lg divide-y divide-surface-elevated">
              {alertEntries.map(([type, count]) => (
                <div key={type} className="flex justify-between items-center px-4 py-3">
                  <span className="text-sm text-text-secondary">{getAlertLabel(type)}</span>
                  <span className="text-sm font-mono text-text-primary">{count}</span>
                </div>
              ))}
              <div className="flex justify-between items-center px-4 py-3 bg-surface-elevated/50">
                <span className="text-sm font-medium text-text-primary">Total</span>
                <span className="text-sm font-mono font-medium text-text-primary">
                  {summary.totalAlerts}
                </span>
              </div>
            </div>
          </div>
        ) : (
          <div className="text-center py-4">
            <div className="w-12 h-12 rounded-full bg-status-ok/20 flex items-center justify-center mx-auto mb-3">
              <svg className="w-6 h-6 text-status-ok" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
              </svg>
            </div>
            <p className="text-sm text-text-secondary">No alerts detected</p>
            <p className="text-xs text-text-muted mt-1">Clean session</p>
          </div>
        )}

        {/* Webhook status */}
        {summary.webhookConfigured && (
          <p className="text-xs text-text-muted text-center">
            Events were delivered to your webhook
          </p>
        )}
      </div>

      {/* Actions */}
      <div className="p-6 space-y-3 border-t border-surface-elevated">
        <button
          onClick={handleRunAgain}
          className="w-full py-2.5 rounded-lg bg-status-info text-white hover:bg-status-info/90 transition-colors text-sm font-medium"
        >
          Run again
        </button>
        <button
          onClick={handleChangePipeline}
          className="w-full py-2.5 rounded-lg bg-surface-elevated text-text-secondary hover:text-text-primary hover:bg-surface-card transition-colors text-sm"
        >
          Change pipeline
        </button>
      </div>
    </div>
  );
}
