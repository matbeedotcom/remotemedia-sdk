import { useEffect } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { useConversationStore } from '../store/conversation';

interface TranscriptionPayload {
  text: string;
  is_final: boolean;
  confidence?: number;
  timestamp_ms: number;
}

interface ResponsePayload {
  text: string;
  model: string;
  streaming: boolean;
  is_final: boolean;
  timestamp_ms: number;
}

interface VadStatePayload {
  active: boolean;
  speaking: boolean;
  probability?: number;
}

interface ModeChangedPayload {
  from: string;
  to: string;
  reason: string;
  details?: string;
}

interface ErrorPayload {
  code: string;
  message: string;
  recoverable: boolean;
  action?: string;
  timestamp_ms: number;
}

export function useTauriEvents() {
  const { addMessage, updateMessage } = useConversationStore();

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    let currentUserMessageId: string | null = null;
    let currentAssistantMessageId: string | null = null;

    // Set up listeners
    const setupListeners = async () => {
      // Transcription events
      unlisteners.push(
        await listen<TranscriptionPayload>('transcription', (event) => {
          const { text, is_final } = event.payload;

          if (is_final) {
            // Add final transcription as user message
            addMessage({
              type: 'user',
              text,
              isPartial: false,
            });
            currentUserMessageId = null;
          } else {
            // Update or create partial message
            if (currentUserMessageId) {
              updateMessage(currentUserMessageId, { text });
            } else {
              currentUserMessageId = addMessage({
                type: 'user',
                text,
                isPartial: true,
              }) as unknown as string;
            }
          }
        })
      );

      // Response events
      unlisteners.push(
        await listen<ResponsePayload>('response', (event) => {
          const { text, is_final } = event.payload;

          if (is_final) {
            if (currentAssistantMessageId) {
              updateMessage(currentAssistantMessageId, { text, isPartial: false });
            } else {
              addMessage({
                type: 'assistant',
                text,
                isPartial: false,
              });
            }
            currentAssistantMessageId = null;
          } else {
            // Streaming response
            if (currentAssistantMessageId) {
              updateMessage(currentAssistantMessageId, { text });
            } else {
              currentAssistantMessageId = addMessage({
                type: 'assistant',
                text,
                isPartial: true,
              }) as unknown as string;
            }
          }
        })
      );

      // VAD state events
      unlisteners.push(
        await listen<VadStatePayload>('vad_state', (event) => {
          console.log('VAD state:', event.payload);
          // VAD state is handled by the component directly for now
        })
      );

      // Mode change events
      unlisteners.push(
        await listen<ModeChangedPayload>('mode_changed', (event) => {
          console.log('Mode changed:', event.payload);
          // Could show a toast notification here
        })
      );

      // Error events
      unlisteners.push(
        await listen<ErrorPayload>('error', (event) => {
          console.error('Error:', event.payload);
          // Could show an error toast here
        })
      );

      // Audio output events
      unlisteners.push(
        await listen('audio_output', (event) => {
          console.log('Audio output:', event.payload);
        })
      );
    };

    setupListeners();

    // Cleanup
    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [addMessage, updateMessage]);
}
