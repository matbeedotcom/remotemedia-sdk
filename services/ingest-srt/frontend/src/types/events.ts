/** Base event from the stream health pipeline */
export interface StreamEvent {
  id: string;
  event_type: string;
  timestamp_us: number;
  _schema?: string;
  [key: string]: unknown;
}

/** Speech presence states */
export type MonoSpeechState = 'speaking' | 'silent' | 'dead_air';
export type StereoSpeechState = 'speaking_left' | 'speaking_right' | 'speaking_both' | 'overlap' | 'silent' | 'dead_air';

/** Speech presence event */
export interface SpeechPresenceEvent extends StreamEvent {
  event_type: 'speech.transition' | 'speech.state';
  state: MonoSpeechState | StereoSpeechState;
  prev_state?: MonoSpeechState | StereoSpeechState;
  duration_ms?: number;
  channel?: 'mono' | 'left' | 'right' | 'both';
}

/** Conversation flow event */
export interface ConversationFlowEvent extends StreamEvent {
  event_type: 'conversation.flow';
  talk_pct?: number;
  left_talk_pct?: number;
  right_talk_pct?: number;
  overlap_pct?: number;
  silence_pct?: number;
  window_ms: number;
}

/** Session health event */
export interface SessionHealthEvent extends StreamEvent {
  event_type: 'session.health';
  status: 'ok' | 'degraded' | 'unhealthy';
  score: number;
  contributors: string[];
}

/** Timing event */
export interface TimingEvent extends StreamEvent {
  event_type: 'timing.report' | 'timing.jitter_spike' | 'timing.clock_drift' | 'timing.lead_jump';
  lead_ms?: number;
  slope_ms_per_s?: number;
  jitter_ms?: number;
  threshold_ms?: number;
}

/** Audio quality events */
export interface SilenceEvent extends StreamEvent {
  event_type: 'audio.silence';
  duration_ms: number;
  threshold_db: number;
}

export interface ClippingEvent extends StreamEvent {
  event_type: 'audio.clipping';
  peak_db: number;
  clipping_frames: number;
  channel?: 'left' | 'right' | 'both';
}

export interface ChannelImbalanceEvent extends StreamEvent {
  event_type: 'audio.channel_imbalance';
  left_db: number;
  right_db: number;
  imbalance_db: number;
}

export interface LowVolumeEvent extends StreamEvent {
  event_type: 'audio.low_volume';
  rms_db: number;
  threshold_db: number;
}

/** Incident (correlated events) */
export interface IncidentEvent extends StreamEvent {
  event_type: 'incident.created' | 'incident.updated' | 'incident.resolved';
  incident_id: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  pattern?: string;
  event_count: number;
  start_us: number;
  end_us?: number;
}

/** System events */
export interface SystemEvent extends StreamEvent {
  event_type: 'stream_started' | 'stream_ended' | 'webhook_delivered';
  message?: string;
}

/** Union of all event types */
export type AnyStreamEvent =
  | SpeechPresenceEvent
  | ConversationFlowEvent
  | SessionHealthEvent
  | TimingEvent
  | SilenceEvent
  | ClippingEvent
  | ChannelImbalanceEvent
  | LowVolumeEvent
  | IncidentEvent
  | SystemEvent
  | StreamEvent;

/** Event category for UI styling */
export type EventCategory =
  | 'speech'
  | 'conversation'
  | 'health'
  | 'timing'
  | 'alert'
  | 'incident'
  | 'system'
  | 'event';

/** Drift/timing alert event types from HealthEmitterNode */
const DRIFT_ALERT_TYPES = new Set(['drift', 'av_skew', 'cadence', 'freeze', 'dropouts']);

/** Get category from event type */
export function getEventCategory(eventType: string): EventCategory {
  if (eventType.startsWith('speech.') || eventType.startsWith('presence.')) return 'speech';
  if (eventType.startsWith('conversation.') || eventType.startsWith('flow.')) return 'conversation';
  if (eventType.startsWith('session.') || eventType === 'session_health') return 'health';
  if (eventType.startsWith('timing.')) return 'timing';
  if (eventType.startsWith('incident.')) return 'incident';
  if (eventType.startsWith('stream_') || eventType.startsWith('webhook')) return 'system';
  // Audio quality and drift/timing alerts
  if (eventType.includes('silence') || eventType.includes('clipping') ||
      eventType.includes('imbalance') || eventType.includes('low_volume') ||
      DRIFT_ALERT_TYPES.has(eventType)) return 'alert';
  return 'event';
}

/** Format timestamp for display */
export function formatTimestamp(timestampUs: number, startUs?: number): string {
  const seconds = startUs
    ? Math.floor((timestampUs - startUs) / 1_000_000)
    : Math.floor(timestampUs / 1_000_000);

  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
}

/** Event group for timeline display */
export interface EventGroup {
  /** The representative event for this group */
  event: AnyStreamEvent;
  /** Number of consecutive events of this type */
  count: number;
  /** Whether this represents multiple events */
  isGrouped: boolean;
}

/** Event types that can be grouped when consecutive */
export const GROUPABLE_EVENT_TYPES = [
  'audio.silence',
  'audio.clipping',
  'audio.low_volume',
  'audio.channel_imbalance',
  'timing.jitter_spike',
  'timing.clock_drift',
] as const;

/** Event types that should NOT be displayed in the timeline (high-frequency status updates) */
const HIDDEN_EVENT_TYPES = new Set([
  'health',              // Health score updates (raw event type from backend)
  'session.health',      // Health score updates (namespaced)
  'timing.report',       // Periodic timing reports - only show anomalies (jitter_spike, clock_drift)
  'conversation.flow',   // Periodic flow snapshots - too noisy for timeline
  'speech.state',        // Continuous speech state - prefer transitions only
]);

/** Check if an event should be displayed in the timeline */
export function isDisplayableEvent(event: AnyStreamEvent): boolean {
  // Filter out routine status updates - they update header/metrics but shouldn't flood timeline
  if (HIDDEN_EVENT_TYPES.has(event.event_type)) return false;
  return true;
}
