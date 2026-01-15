import { create } from 'zustand';
import type { PersonaState } from '@/types/persona';

const STORAGE_KEY = 'remotemedia_persona';

export const usePersonaStore = create<PersonaState>((set) => ({
  activePersona: null,
  source: null,

  setPersona: (persona, source) => {
    set({ activePersona: persona, source });
    if (persona) {
      try {
        sessionStorage.setItem(STORAGE_KEY, persona.slug);
      } catch {
        // sessionStorage not available
      }
    }
  },

  clearPersona: () => {
    set({ activePersona: null, source: null });
    try {
      sessionStorage.removeItem(STORAGE_KEY);
    } catch {
      // sessionStorage not available
    }
  },
}));

/**
 * Get stored persona slug from sessionStorage.
 */
export function getStoredPersonaSlug(): string | null {
  try {
    return sessionStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}
