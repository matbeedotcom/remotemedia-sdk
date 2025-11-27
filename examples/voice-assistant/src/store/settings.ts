import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export interface Settings {
  mode: string;
  remoteServer: string | null;
  llmModel: string;
  ttsVoice: string;
  vadThreshold: number;
  autoListen: boolean;
}

interface SettingsState {
  settings: Settings;
  mode: string;
  updateSettings: (settings: Partial<Settings>) => void;
  setMode: (mode: string) => void;
}

const defaultSettings: Settings = {
  mode: 'local',
  remoteServer: null,
  llmModel: 'llama3.2:1b',
  ttsVoice: 'af_bella',
  vadThreshold: 0.5,
  autoListen: true,
};

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      settings: defaultSettings,
      mode: defaultSettings.mode,

      updateSettings: (updates) => {
        set((state) => ({
          settings: { ...state.settings, ...updates },
          mode: updates.mode ?? state.mode,
        }));
      },

      setMode: (mode) => {
        set((state) => ({
          mode,
          settings: { ...state.settings, mode },
        }));
      },
    }),
    {
      name: 'voice-assistant-settings',
    }
  )
);
