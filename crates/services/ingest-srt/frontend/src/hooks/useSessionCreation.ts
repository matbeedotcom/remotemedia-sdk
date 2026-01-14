import { useState, useEffect, useCallback } from 'react';
import { useSessionStore } from '@/store/session';
import type { Session, CreateSessionRequest, PipelineTemplate } from '@/types/session';

const API_BASE = '';

interface UseSessionCreationResult {
  isCreating: boolean;
  error: string | null;
}

/**
 * Hook that automatically creates a session when a pipeline is selected.
 * Must be used in a component that stays mounted (like App).
 */
export function useSessionCreation(): UseSessionCreationResult {
  const session = useSessionStore((s) => s.session);
  const setSession = useSessionStore((s) => s.setSession);
  const selectedPipeline = useSessionStore((s) => s.selectedPipeline);
  const selectPipeline = useSessionStore((s) => s.selectPipeline);

  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const createSession = useCallback(async (pipeline: PipelineTemplate) => {
    setIsCreating(true);
    setError(null);

    try {
      const request: CreateSessionRequest = {
        pipeline: pipeline.id,
        audio_enabled: true,
        video_enabled: false,
        max_duration_seconds: 300,
      };

      const response = await fetch(`${API_BASE}/api/ingest/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(request),
      });

      if (!response.ok) {
        const err = await response.json();
        throw new Error(err.error || 'Failed to create session');
      }

      const sessionData: Session = await response.json();
      setSession(sessionData);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
      // Reset pipeline selection on error so user can try again
      selectPipeline(null);
    } finally {
      setIsCreating(false);
    }
  }, [setSession, selectPipeline]);

  // Auto-create session when pipeline is selected
  useEffect(() => {
    if (selectedPipeline && !session && !isCreating) {
      createSession(selectedPipeline);
    }
  }, [selectedPipeline, session, isCreating, createSession]);

  return { isCreating, error };
}
