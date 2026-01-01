import { useEffect } from 'react';
import { useSessionStore } from '@/store/session';
import type { GatewayMetrics } from '@/types/session';

const API_BASE = '';
const POLL_INTERVAL = 5000;

/** Hook to poll gateway metrics */
export function useMetrics() {
  const setMetrics = useSessionStore((s) => s.setMetrics);

  useEffect(() => {
    const fetchMetrics = async () => {
      try {
        const response = await fetch(`${API_BASE}/metrics`);
        if (response.ok) {
          const metrics: GatewayMetrics = await response.json();
          setMetrics(metrics);
        }
      } catch (err) {
        console.error('Failed to fetch metrics:', err);
      }
    };

    // Initial fetch
    fetchMetrics();

    // Poll
    const interval = setInterval(fetchMetrics, POLL_INTERVAL);
    return () => clearInterval(interval);
  }, [setMetrics]);
}
