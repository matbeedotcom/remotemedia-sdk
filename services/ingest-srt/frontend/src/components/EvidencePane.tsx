import { useSessionStore } from '@/store/session';
import { getEventCategory, type EventCategory } from '@/types/events';
import clsx from 'clsx';

const categoryColors: Record<EventCategory, string> = {
  speech: 'border-accent-speech',
  conversation: 'border-accent-conversation',
  health: 'border-status-ok',
  timing: 'border-accent-timing',
  alert: 'border-status-warning',
  incident: 'border-status-error',
  system: 'border-text-muted',
  event: 'border-text-secondary',
};

export function EvidencePane() {
  const selectedEventId = useSessionStore((s) => s.selectedEventId);
  const events = useSessionStore((s) => s.events);

  const selectedEvent = selectedEventId ? events.find((e) => e.id === selectedEventId) : null;

  if (!selectedEvent) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-text-muted p-8">
        <p className="text-center">Select an event from the timeline to see details</p>
      </div>
    );
  }

  const category = getEventCategory(selectedEvent.event_type);

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* Header */}
      <div className={clsx('p-4 border-b border-surface-elevated', categoryColors[category])}>
        <h2 className="text-lg font-semibold text-text-primary">{getEventTitle(selectedEvent)}</h2>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {/* Details section */}
        <section>
          <h3 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3">
            Details
          </h3>
          <div className="space-y-2">
            {renderEventDetails(selectedEvent)}
          </div>
        </section>

        {/* Context section (for certain events) */}
        {renderEventContext(selectedEvent)}

        {/* Raw data (collapsed by default) */}
        <section>
          <details className="group">
            <summary className="text-xs font-medium text-text-muted uppercase tracking-wider cursor-pointer hover:text-text-secondary">
              Raw Event Data
            </summary>
            <pre className="mt-2 p-3 bg-surface-primary rounded-lg text-xs font-mono text-text-secondary overflow-x-auto">
              {JSON.stringify(selectedEvent, null, 2)}
            </pre>
          </details>
        </section>
      </div>
    </div>
  );
}

/** Get title for event */
function getEventTitle(event: Record<string, unknown>): string {
  const eventType = event.event_type as string;

  switch (eventType) {
    case 'audio.silence':
      return 'Silence detected';
    case 'audio.clipping':
      return 'Audio clipping detected';
    case 'audio.channel_imbalance':
      return 'Channel imbalance';
    case 'audio.low_volume':
      return 'Low volume detected';
    case 'speech.transition':
    case 'speech.state':
      return 'Speech state change';
    case 'conversation.flow':
      return 'Conversation snapshot';
    case 'session.health':
      return 'Session health update';
    case 'timing.jitter_spike':
      return 'Timing anomaly';
    case 'timing.clock_drift':
      return 'Clock drift';
    case 'timing.report':
      return 'Timing report';
    case 'incident.created':
    case 'incident.updated':
    case 'incident.resolved':
      return 'Incident';
    default:
      return eventType.replace(/[._]/g, ' ');
  }
}

/** Render event-specific details */
function renderEventDetails(event: Record<string, unknown>) {
  const eventType = event.event_type as string;
  const rows: Array<{ label: string; value: string }> = [];

  // Add event-type specific details
  if (eventType === 'audio.silence') {
    if (typeof event.duration_ms === 'number') {
      rows.push({ label: 'Duration', value: `${(event.duration_ms / 1000).toFixed(1)} seconds` });
    }
    if (typeof event.threshold_db === 'number') {
      rows.push({ label: 'Threshold', value: `${event.threshold_db} dB` });
    }
  }

  if (eventType === 'audio.clipping') {
    if (typeof event.peak_db === 'number') {
      rows.push({ label: 'Peak level', value: `${event.peak_db.toFixed(1)} dBFS` });
    }
    if (typeof event.clipping_frames === 'number') {
      rows.push({ label: 'Clipping frames', value: event.clipping_frames.toString() });
    }
    if (event.channel) {
      rows.push({ label: 'Channel', value: event.channel as string });
    }
  }

  if (eventType === 'audio.channel_imbalance') {
    if (typeof event.left_db === 'number') {
      rows.push({ label: 'Left RMS', value: `${event.left_db.toFixed(1)} dB` });
    }
    if (typeof event.right_db === 'number') {
      rows.push({ label: 'Right RMS', value: `${event.right_db.toFixed(1)} dB` });
    }
    if (typeof event.imbalance_db === 'number') {
      rows.push({ label: 'Imbalance', value: `${event.imbalance_db.toFixed(1)} dB` });
    }
  }

  if (eventType === 'conversation.flow') {
    if (typeof event.talk_pct === 'number') {
      rows.push({ label: 'Talk time', value: `${(event.talk_pct * 100).toFixed(1)}%` });
    }
    if (typeof event.left_talk_pct === 'number') {
      rows.push({ label: 'Left channel', value: `${(event.left_talk_pct * 100).toFixed(1)}%` });
    }
    if (typeof event.right_talk_pct === 'number') {
      rows.push({ label: 'Right channel', value: `${(event.right_talk_pct * 100).toFixed(1)}%` });
    }
    if (typeof event.overlap_pct === 'number') {
      rows.push({ label: 'Overlap', value: `${(event.overlap_pct * 100).toFixed(1)}%` });
    }
    if (typeof event.silence_pct === 'number') {
      rows.push({ label: 'Silence', value: `${(event.silence_pct * 100).toFixed(1)}%` });
    }
  }

  if (eventType === 'session.health') {
    if (typeof event.score === 'number') {
      rows.push({ label: 'Health score', value: `${(event.score * 100).toFixed(0)}%` });
    }
    if (event.status) {
      rows.push({ label: 'Status', value: event.status as string });
    }
    if (Array.isArray(event.contributors) && event.contributors.length > 0) {
      rows.push({ label: 'Contributors', value: event.contributors.join(', ') });
    }
  }

  if (eventType.startsWith('timing.')) {
    if (typeof event.jitter_ms === 'number') {
      rows.push({ label: 'Jitter', value: `${event.jitter_ms.toFixed(1)} ms` });
    }
    if (typeof event.lead_ms === 'number') {
      rows.push({ label: 'Lead', value: `${event.lead_ms.toFixed(1)} ms` });
    }
    if (typeof event.slope_ms_per_s === 'number') {
      rows.push({ label: 'Drift rate', value: `${event.slope_ms_per_s.toFixed(2)} ms/s` });
    }
    if (typeof event.threshold_ms === 'number') {
      rows.push({ label: 'Threshold', value: `${event.threshold_ms} ms` });
    }
  }

  if (eventType.startsWith('speech.')) {
    if (event.state) {
      rows.push({ label: 'State', value: event.state as string });
    }
    if (event.prev_state) {
      rows.push({ label: 'Previous state', value: event.prev_state as string });
    }
    if (typeof event.duration_ms === 'number') {
      rows.push({ label: 'Duration', value: `${(event.duration_ms / 1000).toFixed(1)} seconds` });
    }
    if (event.channel) {
      rows.push({ label: 'Channel', value: event.channel as string });
    }
  }

  // Render rows
  return rows.map(({ label, value }) => (
    <div key={label} className="flex justify-between items-baseline py-1.5 border-b border-surface-elevated last:border-0">
      <span className="text-text-secondary text-sm">{label}</span>
      <span className="text-text-primary font-mono text-sm">{value}</span>
    </div>
  ));
}

/** Render contextual information */
function renderEventContext(event: Record<string, unknown>) {
  const eventType = event.event_type as string;
  const hints: string[] = [];

  if (eventType === 'audio.clipping') {
    hints.push('Mic gain may be too high');
    hints.push('Input signal overdriven');
  }

  if (eventType === 'audio.channel_imbalance') {
    hints.push('Mono mic connected to stereo input');
    hints.push('Check cable connections');
  }

  if (eventType === 'audio.silence') {
    hints.push('Microphone may be muted');
    hints.push('No audio source detected');
  }

  if (eventType === 'timing.jitter_spike' || eventType === 'timing.clock_drift') {
    hints.push('Possible network instability');
    hints.push('Check for CPU throttling');
  }

  if (eventType === 'speech.state' && event.state === 'dead_air') {
    hints.push('No active speaker detected');
    hints.push('Extended pause in conversation');
  }

  if (hints.length === 0) return null;

  return (
    <section>
      <h3 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3">
        Likely cause
      </h3>
      <ul className="space-y-1.5">
        {hints.map((hint) => (
          <li key={hint} className="flex items-start gap-2 text-sm text-text-secondary">
            <span className="text-text-muted">•</span>
            <span>{hint}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
