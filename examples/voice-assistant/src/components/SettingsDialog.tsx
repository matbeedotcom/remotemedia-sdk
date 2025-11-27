import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { X } from 'lucide-react';
import { useSettingsStore, Settings } from '../store/settings';

interface SettingsDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

export function SettingsDialog({ isOpen, onClose }: SettingsDialogProps) {
  const { settings, updateSettings } = useSettingsStore();
  const [localSettings, setLocalSettings] = useState<Settings>(settings);

  useEffect(() => {
    setLocalSettings(settings);
  }, [settings, isOpen]);

  if (!isOpen) {
    return null;
  }

  const handleSave = async () => {
    try {
      await invoke('update_settings', { settings: localSettings });
      updateSettings(localSettings);
      onClose();
    } catch (error) {
      console.error('Failed to save settings:', error);
    }
  };

  const handleCancel = () => {
    setLocalSettings(settings);
    onClose();
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-gray-800 rounded-lg w-full max-w-md mx-4 overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-700">
          <h2 className="text-lg font-semibold">Settings</h2>
          <button
            onClick={handleCancel}
            className="p-1 rounded hover:bg-gray-700"
            aria-label="Close"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-4 space-y-4">
          {/* Mode */}
          <div>
            <label className="block text-sm font-medium mb-1">Execution Mode</label>
            <select
              value={localSettings.mode}
              onChange={(e) => setLocalSettings({ ...localSettings, mode: e.target.value })}
              className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:border-blue-500"
            >
              <option value="local">Local (all on device)</option>
              <option value="hybrid">Hybrid (local VAD + remote inference)</option>
              <option value="remote">Remote (all on server)</option>
            </select>
          </div>

          {/* Remote Server */}
          {localSettings.mode !== 'local' && (
            <div>
              <label className="block text-sm font-medium mb-1">Remote Server URL</label>
              <input
                type="text"
                value={localSettings.remoteServer || ''}
                onChange={(e) => setLocalSettings({ ...localSettings, remoteServer: e.target.value })}
                placeholder="grpc://localhost:50051"
                className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:border-blue-500"
              />
            </div>
          )}

          {/* LLM Model */}
          <div>
            <label className="block text-sm font-medium mb-1">LLM Model</label>
            <input
              type="text"
              value={localSettings.llmModel}
              onChange={(e) => setLocalSettings({ ...localSettings, llmModel: e.target.value })}
              placeholder="llama3.2:1b"
              className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:border-blue-500"
            />
          </div>

          {/* TTS Voice */}
          <div>
            <label className="block text-sm font-medium mb-1">TTS Voice</label>
            <select
              value={localSettings.ttsVoice}
              onChange={(e) => setLocalSettings({ ...localSettings, ttsVoice: e.target.value })}
              className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:border-blue-500"
            >
              <option value="af_bella">Bella (Female)</option>
              <option value="af_nicole">Nicole (Female)</option>
              <option value="am_adam">Adam (Male)</option>
              <option value="am_michael">Michael (Male)</option>
            </select>
          </div>

          {/* VAD Threshold */}
          <div>
            <label className="block text-sm font-medium mb-1">
              VAD Threshold: {localSettings.vadThreshold}
            </label>
            <input
              type="range"
              min="0.1"
              max="0.9"
              step="0.1"
              value={localSettings.vadThreshold}
              onChange={(e) => setLocalSettings({ ...localSettings, vadThreshold: parseFloat(e.target.value) })}
              className="w-full"
            />
          </div>

          {/* Auto-listen */}
          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="autoListen"
              checked={localSettings.autoListen}
              onChange={(e) => setLocalSettings({ ...localSettings, autoListen: e.target.checked })}
              className="w-4 h-4 rounded"
            />
            <label htmlFor="autoListen" className="text-sm">
              Auto-listen after response
            </label>
          </div>
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-gray-700">
          <button
            onClick={handleCancel}
            className="px-4 py-2 text-gray-300 hover:text-white transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-lg transition-colors"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
