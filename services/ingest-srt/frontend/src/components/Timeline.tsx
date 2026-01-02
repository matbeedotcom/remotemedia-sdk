import { useMemo } from 'react';
import { useSessionStore } from '@/store/session';
import { getEventCategory, formatTimestamp, isDisplayableEvent, type AnyStreamEvent, type EventCategory } from '@/types/events';
import { useEventCopy } from '@/hooks/useEventCopy';
import clsx from 'clsx';

/** Color mapping for event categories */
const categoryColors: Record<EventCategory, string> = {
  speech: 'bg-accent-speech',
  conversation: 'bg-accent-conversation',
  health: 'bg-status-ok',
  timing: 'bg-accent-timing',
  alert: 'bg-status-warning',
  incident: 'bg-status-error',
  system: 'bg-text-muted',
  event: 'bg-text-secondary',
};

/** Event types that should be grouped when consecutive */
const GROUPABLE_EVENTS = new Set([
  'audio.low_volume',
  'audio.silence',
  'audio.clipping',
  'audio.channel_imbalance',
  'timing.jitter_spike',
  'timing.clock_drift',
  // Drift/timing alerts from HealthEmitterNode
  'drift',
  'av_skew',
  'cadence',
  'freeze',
  'dropouts',
  'silence',
  'clipping',
  'low_volume',
  'channel_imbalance',
]);

/** Grouped event with count */
interface GroupedEvent {
  event: AnyStreamEvent;
  count: number;
  firstTimestampUs: number;
  lastTimestampUs: number;
}

/** Group consecutive events of the same type */
function groupConsecutiveEvents(events: AnyStreamEvent[]): GroupedEvent[] {
  if (events.length === 0) return [];

  const groups: GroupedEvent[] = [];
  let currentGroup: GroupedEvent | null = null;

  for (const event of events) {
    const isGroupable = GROUPABLE_EVENTS.has(event.event_type);

    if (
      currentGroup &&
      isGroupable &&
      currentGroup.event.event_type === event.event_type
    ) {
      // Add to existing group
      currentGroup.count++;
      currentGroup.lastTimestampUs = event.timestamp_us;
    } else {
      // Start new group
      currentGroup = {
        event,
        count: 1,
        firstTimestampUs: event.timestamp_us,
        lastTimestampUs: event.timestamp_us,
      };
      groups.push(currentGroup);
    }
  }

  return groups;
}

export function Timeline() {
  const events = useSessionStore((s) => s.events);
  const selectedEventId = useSessionStore((s) => s.selectedEventId);
  const selectEvent = useSessionStore((s) => s.selectEvent);
  const startedAt = useSessionStore((s) => s.startedAt);
  const { getEventLabel } = useEventCopy();

  const startUs = startedAt ? startedAt * 1000 : undefined;

  // Filter to displayable events and group consecutive events of the same type
  const groupedEvents = useMemo(() => {
    const displayable = events.filter(isDisplayableEvent);
    return groupConsecutiveEvents(displayable);
  }, [events]);

  if (events.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-text-muted">
        <p>Events will appear here when the stream starts</p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto px-4 py-4">
      <div className="relative">
        {/* Timeline line */}
        <div className="absolute left-[7px] top-0 bottom-0 w-[2px] bg-surface-elevated" />

        {/* Events */}
        <div className="space-y-2">
          {groupedEvents.map((group) => (
            <TimelineEvent
              key={group.event.id}
              event={group.event}
              count={group.count}
              isSelected={group.event.id === selectedEventId}
              onSelect={() => selectEvent(group.event.id === selectedEventId ? null : group.event.id)}
              startUs={startUs}
              getEventLabel={getEventLabel}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

interface TimelineEventProps {
  event: AnyStreamEvent;
  count: number;
  isSelected: boolean;
  onSelect: () => void;
  startUs?: number;
  getEventLabel: (eventType: string) => string;
}

function TimelineEvent({ event, count, isSelected, onSelect, startUs, getEventLabel }: TimelineEventProps) {
  const category = getEventCategory(event.event_type);
  const time = formatTimestamp(event.timestamp_us, startUs);

  // Get human-readable label (persona-aware via hook)
  const label = getLabel(event, getEventLabel);
  const detail = getEventDetail(event);

  return (
    <button
      onClick={onSelect}
      className={clsx(
        'w-full text-left flex items-start gap-3 p-2 rounded-lg transition-all',
        'hover:bg-surface-elevated/50',
        isSelected && 'bg-surface-elevated ring-1 ring-text-muted/30'
      )}
    >
      {/* Dot with count badge */}
      <div className="relative flex-shrink-0">
        <div
          className={clsx(
            'w-4 h-4 rounded-full mt-0.5 ring-2 ring-surface-primary',
            categoryColors[category]
          )}
        />
        {count > 1 && (
          <span className="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 flex items-center justify-center text-[10px] font-bold bg-surface-elevated text-text-primary rounded-full border border-surface-primary">
            {count > 99 ? '99+' : count}
          </span>
        )}
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="flex items-baseline justify-between gap-2">
          <span className="text-sm font-medium text-text-primary truncate">
            {label}
            {count > 1 && <span className="text-text-muted font-normal"> ({count}x)</span>}
          </span>
          <span className="text-xs text-text-muted font-mono flex-shrink-0">{time}</span>
        </div>
        {detail && <p className="text-xs text-text-secondary mt-0.5 truncate">{detail}</p>}
      </div>
    </button>
  );
}

/** Get human-readable label for event using persona-aware copy */
function getLabel(event: AnyStreamEvent, getEventLabel: (eventType: string) => string): string {
  // Speech events need special handling for state-based labels
  if (event.event_type === 'speech.transition' || event.event_type === 'speech.state') {
    return getSpeechLabel(event);
  }

  // Session health includes status in label
  if (event.event_type === 'session.health') {
    return `Session ${(event as { status: string }).status}`;
  }

  // Use persona-aware label for all other events
  return getEventLabel(event.event_type);
}

/** Get detail text for event */
function getEventDetail(event: AnyStreamEvent): string | null {
  const e = event as Record<string, unknown>;

  if (event.event_type === 'audio.silence' && typeof e.duration_ms === 'number') {
    return `${(e.duration_ms / 1000).toFixed(1)}s`;
  }
  if (event.event_type === 'conversation.flow') {
    const parts: string[] = [];
    if (typeof e.talk_pct === 'number') parts.push(`Talk: ${(e.talk_pct * 100).toFixed(0)}%`);
    if (typeof e.silence_pct === 'number') parts.push(`Silence: ${(e.silence_pct * 100).toFixed(0)}%`);
    return parts.join(' · ') || null;
  }
  if (event.event_type === 'timing.jitter_spike' && typeof e.jitter_ms === 'number') {
    return `${e.jitter_ms.toFixed(0)}ms`;
  }
  if (event.event_type === 'session.health' && typeof e.score === 'number') {
    return `Score: ${(e.score * 100).toFixed(0)}%`;
  }
  // Drift/timing alerts from HealthEmitterNode
  if (event.event_type === 'drift' && typeof e.lead_ms === 'number') {
    return `Lead: ${e.lead_ms}ms`;
  }
  if (event.event_type === 'av_skew' && typeof e.skew_ms === 'number') {
    return `Skew: ${e.skew_ms}ms`;
  }
  if (event.event_type === 'cadence' && typeof e.cv === 'number') {
    return `CV: ${(e.cv * 100).toFixed(0)}%`;
  }
  if (event.event_type === 'freeze' && typeof e.duration_ms === 'number') {
    return `${(e.duration_ms / 1000).toFixed(1)}s`;
  }
  if (event.event_type === 'silence' && typeof e.duration_ms === 'number') {
    return `${(e.duration_ms / 1000).toFixed(1)}s`;
  }
  if (event.event_type === 'health' && typeof e.score === 'number') {
    return `Score: ${(e.score * 100).toFixed(0)}%`;
  }

  return null;
}

/** Get speech event label */
function getSpeechLabel(event: AnyStreamEvent): string {
  const e = event as { state?: string; duration_ms?: number };
  const state = e.state || 'unknown';

  switch (state) {
    case 'speaking':
      return 'Speech detected';
    case 'silent':
      return 'Silence';
    case 'dead_air':
      return 'Dead air (no active speaker)';
    case 'speaking_left':
      return 'Left channel speaking';
    case 'speaking_right':
      return 'Right channel speaking';
    case 'speaking_both':
      return 'Both channels speaking';
    case 'overlap':
      return 'Crosstalk detected';
    default:
      return `Speech: ${state}`;
  }
}
