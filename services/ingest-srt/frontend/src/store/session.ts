import { create } from 'zustand';
import type { Session, SessionStatus, GatewayMetrics, PipelineTemplate } from '@/types/session';
import type { AnyStreamEvent, IncidentEvent } from '@/types/events';

/** Grouped incident with its child events */
export interface Incident {
  id: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  pattern?: string;
  events: AnyStreamEvent[];
  startUs: number;
  endUs?: number;
  resolved: boolean;
}

interface SessionState {
  // Session
  session: Session | null;
  status: SessionStatus;
  startedAt: number | null;

  // Pipeline selection (before session creation)
  selectedPipeline: PipelineTemplate | null;

  // Command lock state (after stream connects)
  commandLocked: boolean;

  // Events
  events: AnyStreamEvent[];
  selectedEventId: string | null;

  // Incidents (correlated events)
  incidents: Incident[];

  // Health
  healthScore: number | null;
  healthStatus: 'ok' | 'degraded' | 'unhealthy' | null;
  activeContributors: string[];

  // Metrics
  metrics: GatewayMetrics | null;

  // Actions
  setSession: (session: Session | null) => void;
  setStatus: (status: SessionStatus) => void;
  addEvent: (event: AnyStreamEvent) => void;
  selectEvent: (eventId: string | null) => void;
  updateHealth: (score: number, status: 'ok' | 'degraded' | 'unhealthy', contributors: string[]) => void;
  setMetrics: (metrics: GatewayMetrics) => void;
  selectPipeline: (pipeline: PipelineTemplate | null) => void;
  lockCommand: () => void;
  reset: () => void;
}

const MAX_EVENTS = 200;

export const useSessionStore = create<SessionState>((set, get) => ({
  // Initial state
  session: null,
  status: 'idle',
  startedAt: null,
  selectedPipeline: null,
  commandLocked: false,
  events: [],
  selectedEventId: null,
  incidents: [],
  healthScore: null,
  healthStatus: null,
  activeContributors: [],
  metrics: null,

  // Actions
  setSession: (session) =>
    set({
      session,
      status: session ? 'created' : 'idle',
      startedAt: session ? Date.now() : null,
    }),

  setStatus: (status) => set({ status }),

  addEvent: (event) => {
    const state = get();
    const events = [event, ...state.events].slice(0, MAX_EVENTS);

    // Handle incident events
    let incidents = state.incidents;
    if (event.event_type.startsWith('incident.')) {
      const incidentEvent = event as IncidentEvent;
      incidents = handleIncidentEvent(incidents, incidentEvent, event);
    }

    // Update health if it's a session health event
    if (event.event_type === 'session.health') {
      const healthEvent = event as { score: number; status: 'ok' | 'degraded' | 'unhealthy'; contributors: string[] };
      set({
        events,
        incidents,
        healthScore: healthEvent.score,
        healthStatus: healthEvent.status,
        activeContributors: healthEvent.contributors || [],
      });
    } else {
      set({ events, incidents });
    }

    // Update status based on system events
    if (event.event_type === 'stream_started') {
      set({ status: 'streaming', commandLocked: true });
    } else if (event.event_type === 'stream_ended') {
      set({ status: 'disconnected' });
    }
  },

  selectEvent: (eventId) => set({ selectedEventId: eventId }),

  updateHealth: (score, status, contributors) =>
    set({
      healthScore: score,
      healthStatus: status,
      activeContributors: contributors,
    }),

  setMetrics: (metrics) => set({ metrics }),

  selectPipeline: (pipeline) => set({ selectedPipeline: pipeline }),

  lockCommand: () => set({ commandLocked: true }),

  reset: () =>
    set({
      session: null,
      status: 'idle',
      startedAt: null,
      selectedPipeline: null,
      commandLocked: false,
      events: [],
      selectedEventId: null,
      incidents: [],
      healthScore: null,
      healthStatus: null,
      activeContributors: [],
    }),
}));

/** Handle incident event updates */
function handleIncidentEvent(
  incidents: Incident[],
  incidentEvent: IncidentEvent,
  rawEvent: AnyStreamEvent
): Incident[] {
  const existing = incidents.find((i) => i.id === incidentEvent.incident_id);

  if (incidentEvent.event_type === 'incident.created') {
    if (existing) return incidents;
    return [
      {
        id: incidentEvent.incident_id,
        severity: incidentEvent.severity,
        pattern: incidentEvent.pattern,
        events: [rawEvent],
        startUs: incidentEvent.start_us,
        resolved: false,
      },
      ...incidents,
    ];
  }

  if (incidentEvent.event_type === 'incident.updated' && existing) {
    return incidents.map((i) =>
      i.id === incidentEvent.incident_id
        ? { ...i, events: [...i.events, rawEvent], severity: incidentEvent.severity }
        : i
    );
  }

  if (incidentEvent.event_type === 'incident.resolved' && existing) {
    return incidents.map((i) =>
      i.id === incidentEvent.incident_id
        ? { ...i, resolved: true, endUs: incidentEvent.end_us }
        : i
    );
  }

  return incidents;
}
