/**
 * Mapping from event type strings to persona-specific display labels.
 */
export type EventCopyMap = Record<string, string>;

/**
 * Hero section content for landing page.
 */
export interface PersonaHero {
  headline: string;
  subheadline: string;
}

/**
 * Problem section content for landing page.
 */
export interface PersonaProblem {
  title: string;
  bullets: string[];
}

/**
 * Demo preview section content for landing page.
 */
export interface PersonaDemoShows {
  title: string;
  bullets: string[];
}

/**
 * Complete persona configuration including content and behavior mappings.
 */
export interface Persona {
  /** Unique identifier for the persona */
  id: string;

  /** URL slug for routing (e.g., "voice-agents") */
  slug: string;

  /** Display name for UI */
  name: string;

  /** Pipeline ID to preselect when entering observer UI */
  defaultPipeline: string;

  /** Hero section content */
  hero: PersonaHero;

  /** Problem section content */
  problem: PersonaProblem;

  /** Demo preview section content */
  demoShows: PersonaDemoShows;

  /** Event type to persona-specific display copy mapping */
  eventCopyMap: EventCopyMap;
}

/**
 * Persona store state for runtime context management.
 */
export interface PersonaState {
  /** Currently active persona, null if none selected */
  activePersona: Persona | null;

  /** Source of the current persona context */
  source: 'url' | 'storage' | 'default' | null;

  /** Set the active persona with source tracking */
  setPersona: (persona: Persona | null, source: PersonaState['source']) => void;

  /** Clear the active persona context */
  clearPersona: () => void;
}
