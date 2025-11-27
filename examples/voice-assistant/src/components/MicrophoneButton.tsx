import { Mic, MicOff } from 'lucide-react';

interface MicrophoneButtonProps {
  isListening: boolean;
  onClick: () => void;
  disabled?: boolean;
}

export function MicrophoneButton({ isListening, onClick, disabled }: MicrophoneButtonProps) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`
        relative w-16 h-16 rounded-full
        flex items-center justify-center
        transition-all duration-200
        ${isListening
          ? 'bg-red-500 hover:bg-red-600 animate-pulse'
          : 'bg-blue-500 hover:bg-blue-600'
        }
        ${disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}
        focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500
      `}
      aria-label={isListening ? 'Stop listening' : 'Start listening'}
    >
      {isListening ? (
        <MicOff className="w-8 h-8 text-white" />
      ) : (
        <Mic className="w-8 h-8 text-white" />
      )}

      {/* Pulse ring when listening */}
      {isListening && (
        <span className="absolute inset-0 rounded-full border-4 border-red-400 animate-ping opacity-75" />
      )}
    </button>
  );
}
