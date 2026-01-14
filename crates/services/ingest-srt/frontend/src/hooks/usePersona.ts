import { useEffect } from 'react';
import { useSearchParams } from 'react-router-dom';
import { usePersonaStore, getStoredPersonaSlug } from '@/store/persona';
import { getPersonaBySlug } from '@/config/personas';
import { useSessionStore } from '@/store/session';
import { PIPELINES } from '@/types/session';

/**
 * Hook to initialize persona context from URL params or sessionStorage.
 * Should be called once at observer UI mount.
 */
export function usePersonaFromURL() {
  const [searchParams] = useSearchParams();
  const { activePersona, setPersona } = usePersonaStore();
  const selectPipeline = useSessionStore((s) => s.selectPipeline);
  const selectedPipeline = useSessionStore((s) => s.selectedPipeline);

  useEffect(() => {
    // Already have a persona - skip initialization
    if (activePersona) return;

    // Try URL param first
    const urlPersonaSlug = searchParams.get('persona');
    if (urlPersonaSlug) {
      const persona = getPersonaBySlug(urlPersonaSlug);
      if (persona) {
        setPersona(persona, 'url');

        // Auto-select default pipeline if none selected
        if (!selectedPipeline) {
          const defaultPipeline = PIPELINES.find(
            (p) => p.id === persona.defaultPipeline
          );
          if (defaultPipeline) {
            selectPipeline(defaultPipeline);
          }
        }
        return;
      }
    }

    // Fall back to sessionStorage
    const storedSlug = getStoredPersonaSlug();
    if (storedSlug) {
      const persona = getPersonaBySlug(storedSlug);
      if (persona) {
        setPersona(persona, 'storage');

        // Auto-select default pipeline if none selected
        if (!selectedPipeline) {
          const defaultPipeline = PIPELINES.find(
            (p) => p.id === persona.defaultPipeline
          );
          if (defaultPipeline) {
            selectPipeline(defaultPipeline);
          }
        }
      }
    }
  }, [searchParams, activePersona, setPersona, selectedPipeline, selectPipeline]);

  return { activePersona };
}

/**
 * Hook to check if we're in demo mode.
 */
export function useDemoMode() {
  const [searchParams] = useSearchParams();
  return searchParams.get('demo') === 'true';
}
