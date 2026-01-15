import { create } from 'zustand';

export interface ProgressEvent {
  node_type: string;
  node_id: string | null;
  event_type: 'download_started' | 'download_progress' | 'download_complete' | 
              'loading_started' | 'loading_progress' | 'loading_complete' | 
              'init_complete' | 'error';
  message: string;
  progress_pct: number | null;
  details: Record<string, unknown> | null;
}

interface ProgressState {
  /** Current progress events by node type */
  events: Record<string, ProgressEvent>;
  /** Whether any model is currently loading/downloading */
  isLoading: boolean;
  /** Overall progress message to display */
  statusMessage: string | null;
  
  /** Update progress for a node */
  updateProgress: (event: ProgressEvent) => void;
  /** Clear all progress */
  clearProgress: () => void;
}

export const useProgressStore = create<ProgressState>((set) => ({
  events: {},
  isLoading: false,
  statusMessage: null,
  
  updateProgress: (event) => set((state) => {
    const newEvents = { ...state.events, [event.node_type]: event };
    
    // Determine if we're still loading
    const isLoading = Object.values(newEvents).some(
      e => e.event_type !== 'init_complete' && e.event_type !== 'error'
    );
    
    // Build status message from active events
    const activeEvents = Object.values(newEvents).filter(
      e => e.event_type !== 'init_complete' && e.event_type !== 'error'
    );
    
    let statusMessage: string | null = null;
    if (activeEvents.length > 0) {
      const latestEvent = activeEvents[activeEvents.length - 1];
      if (latestEvent.progress_pct !== null) {
        statusMessage = `${latestEvent.message} (${latestEvent.progress_pct.toFixed(0)}%)`;
      } else {
        statusMessage = latestEvent.message;
      }
    }
    
    return {
      events: newEvents,
      isLoading,
      statusMessage,
    };
  }),
  
  clearProgress: () => set({
    events: {},
    isLoading: false,
    statusMessage: null,
  }),
}));
