import { Monitor, Cloud, Combine } from 'lucide-react';

interface ModeSelectorProps {
  currentMode: string;
  onModeChange: (mode: string) => void;
}

const modes = [
  { id: 'local', label: 'Local', icon: Monitor, description: 'All processing on device' },
  { id: 'hybrid', label: 'Hybrid', icon: Combine, description: 'VAD local, inference remote' },
  { id: 'remote', label: 'Remote', icon: Cloud, description: 'All processing on server' },
];

export function ModeSelector({ currentMode, onModeChange }: ModeSelectorProps) {
  return (
    <div className="flex items-center gap-1 bg-gray-800 rounded-lg p-1">
      {modes.map(({ id, label, icon: Icon }) => (
        <button
          key={id}
          onClick={() => onModeChange(id)}
          className={`
            flex items-center gap-1 px-3 py-1.5 rounded-md text-sm
            transition-colors duration-150
            ${currentMode === id
              ? 'bg-blue-600 text-white'
              : 'text-gray-400 hover:text-white hover:bg-gray-700'
            }
          `}
          title={modes.find(m => m.id === id)?.description}
        >
          <Icon className="w-4 h-4" />
          <span className="hidden sm:inline">{label}</span>
        </button>
      ))}
    </div>
  );
}
