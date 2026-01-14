import { useEffect, useRef, useCallback } from 'react';
import { useSessionStore } from '@/store/session';
import type { AnyStreamEvent } from '@/types/events';

const API_BASE = '';

/** Hook to manage SSE connection for session events */
export function useSSE(sessionId: string | null) {
  const eventSourceRef = useRef<EventSource | null>(null);
  const addEvent = useSessionStore((s) => s.addEvent);
  const setStatus = useSessionStore((s) => s.setStatus);

  const connect = useCallback(() => {
    if (!sessionId) return;

    // Close existing connection
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
    }

    const eventSource = new EventSource(`${API_BASE}/api/ingest/sessions/${sessionId}/events`);
    eventSourceRef.current = eventSource;

    // Handle different event types from the server
    const eventTypes = ['alert', 'health', 'system', 'event'];

    eventTypes.forEach((type) => {
      eventSource.addEventListener(type, (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data);
          const event: AnyStreamEvent = {
            id: crypto.randomUUID(),
            timestamp_us: data.timestamp_us || Date.now() * 1000,
            event_type: data.event_type || data.type || type,
            ...data,
          };
          addEvent(event);
        } catch (err) {
          console.error('Failed to parse SSE event:', err);
        }
      });
    });

    eventSource.onerror = () => {
      console.error('SSE connection error');
      setStatus('disconnected');
    };

    eventSource.onopen = () => {
      setStatus('connecting');
    };
  }, [sessionId, addEvent, setStatus]);

  const disconnect = useCallback(() => {
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }
  }, []);

  // Connect when sessionId changes
  useEffect(() => {
    if (sessionId) {
      connect();
    }
    return () => disconnect();
  }, [sessionId, connect, disconnect]);

  return { connect, disconnect };
}
