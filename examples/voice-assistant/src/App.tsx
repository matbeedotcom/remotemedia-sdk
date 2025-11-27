import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { MicrophoneButton } from './components/MicrophoneButton';
import { TranscriptPanel } from './components/TranscriptPanel';
import { VoiceIndicator } from './components/VoiceIndicator';
import { SettingsDialog } from './components/SettingsDialog';
import { ModeSelector } from './components/ModeSelector';
import { ConnectionStatus } from './components/ConnectionStatus';
import { useTauriEvents } from './hooks/useTauriEvents';
import { useConversationStore } from './store/conversation';
import { useSettingsStore } from './store/settings';
import { Settings } from 'lucide-react';

function App() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isListening, setIsListening] = useState(false);
  const [isInitialized, setIsInitialized] = useState(false);

  const { messages, addMessage } = useConversationStore();
  const { settings, mode, setMode } = useSettingsStore();

  // Set up Tauri event listeners
  useTauriEvents();

  // Initialize pipeline on mount
  useEffect(() => {
    const initPipeline = async () => {
      try {
        await invoke('initialize_pipeline', {
          mode: settings.mode,
          remoteServer: settings.remoteServer,
        });
        setIsInitialized(true);
      } catch (error) {
        console.error('Failed to initialize pipeline:', error);
      }
    };

    initPipeline();
  }, []);

  const handleMicrophoneClick = async () => {
    try {
      if (isListening) {
        await invoke('stop_listening');
        setIsListening(false);
      } else {
        await invoke('start_listening');
        setIsListening(true);
      }
    } catch (error) {
      console.error('Audio error:', error);
    }
  };

  const handleModeChange = async (newMode: string) => {
    try {
      await invoke('shutdown_pipeline');
      await invoke('initialize_pipeline', {
        mode: newMode,
        remoteServer: settings.remoteServer,
      });
      setMode(newMode);
    } catch (error) {
      console.error('Failed to change mode:', error);
    }
  };

  const handleTextSubmit = async (text: string) => {
    try {
      await invoke('send_text_input', { text });
    } catch (error) {
      console.error('Failed to send text:', error);
    }
  };

  return (
    <div className="flex flex-col h-screen bg-gray-900 text-white">
      {/* Header */}
      <header className="flex items-center justify-between px-4 py-3 border-b border-gray-700">
        <h1 className="text-xl font-semibold">Voice Assistant</h1>
        <div className="flex items-center gap-4">
          <ConnectionStatus mode={mode} />
          <ModeSelector
            currentMode={mode}
            onModeChange={handleModeChange}
          />
          <button
            onClick={() => setIsSettingsOpen(true)}
            className="p-2 rounded-lg hover:bg-gray-700 transition-colors"
            aria-label="Settings"
          >
            <Settings className="w-5 h-5" />
          </button>
        </div>
      </header>

      {/* Main content */}
      <main className="flex-1 flex flex-col overflow-hidden">
        {/* Transcript panel */}
        <TranscriptPanel
          messages={messages}
          className="flex-1 overflow-y-auto"
        />

        {/* Voice indicator and controls */}
        <div className="flex flex-col items-center gap-4 p-6 border-t border-gray-700">
          <VoiceIndicator isActive={isListening} />

          <div className="flex items-center gap-4">
            <MicrophoneButton
              isListening={isListening}
              onClick={handleMicrophoneClick}
              disabled={!isInitialized}
            />
          </div>

          {/* Text input */}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const input = e.currentTarget.elements.namedItem('text') as HTMLInputElement;
              if (input.value.trim()) {
                handleTextSubmit(input.value.trim());
                input.value = '';
              }
            }}
            className="w-full max-w-2xl"
          >
            <input
              type="text"
              name="text"
              placeholder="Or type your message..."
              className="w-full px-4 py-2 bg-gray-800 border border-gray-600 rounded-lg focus:outline-none focus:border-blue-500"
              disabled={!isInitialized}
            />
          </form>
        </div>
      </main>

      {/* Settings dialog */}
      <SettingsDialog
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
      />
    </div>
  );
}

export default App;
