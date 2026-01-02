import { useCallback } from 'react';
import { usePersonaStore } from '@/store/persona';
import type { EventCopyMap } from '@/types/persona';

/**
 * Default labels for events (used when no persona transform exists).
 */
const DEFAULT_LABELS: Record<string, string> = {
  // System events
  'stream_started': 'Stream started',
  'stream_ended': 'Stream ended',
  'webhook_delivered': 'Webhook delivered',
  // Audio quality alerts
  'audio.silence': 'Silence detected',
  'audio.clipping': 'Audio clipping detected',
  'audio.low_volume': 'Low volume detected',
  'audio.channel_imbalance': 'Channel imbalance detected',
  'silence': 'Silence detected',
  'clipping': 'Audio clipping',
  'low_volume': 'Low volume',
  'channel_imbalance': 'Channel imbalance',
  'dropouts': 'Audio dropouts',
  // Drift/timing alerts (from HealthEmitterNode)
  'drift': 'A/V drift detected',
  'av_skew': 'A/V skew detected',
  'cadence': 'Frame cadence unstable',
  'freeze': 'Stream freeze detected',
  // Timing events
  'timing.jitter_spike': 'Timing jitter spike',
  'timing.clock_drift': 'Clock drift detected',
  'timing.lead_jump': 'Lead time jump',
  'timing.report': 'Timing report',
  // Speech/conversation events
  'speech.transition': 'Speech transition',
  'speech.state': 'Speech state change',
  'conversation.flow': 'Conversation snapshot',
  // Health events
  'session.health': 'Session health',
  'health': 'Health update',
  // Incident events
  'incident.created': 'Incident started',
  'incident.resolved': 'Incident resolved',
};

/**
 * Hook that provides persona-aware event label transformation.
 *
 * Timeline components use this to show persona-specific labels,
 * while EvidencePane shows raw event types for technical users.
 */
export function useEventCopy() {
  const activePersona = usePersonaStore((s) => s.activePersona);
  const eventCopyMap: EventCopyMap = activePersona?.eventCopyMap ?? {};

  /**
   * Get the display label for an event type.
   * Returns persona-specific label if available, otherwise default label.
   */
  const getEventLabel = useCallback(
    (eventType: string): string => {
      // First check persona-specific copy
      if (eventCopyMap[eventType]) {
        return eventCopyMap[eventType];
      }

      // Fall back to default labels
      if (DEFAULT_LABELS[eventType]) {
        return DEFAULT_LABELS[eventType];
      }

      // Last resort: format the event type as readable text
      return eventType.replace(/[._]/g, ' ');
    },
    [eventCopyMap]
  );

  return { getEventLabel };
}
