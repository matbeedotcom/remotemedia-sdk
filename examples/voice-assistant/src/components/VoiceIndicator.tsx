interface VoiceIndicatorProps {
  isActive: boolean;
  isSpeaking?: boolean;
  probability?: number;
}

export function VoiceIndicator({ isActive, isSpeaking, probability }: VoiceIndicatorProps) {
  if (!isActive) {
    return null;
  }

  return (
    <div className="flex items-center gap-2">
      {/* Animated bars */}
      <div className="flex items-end gap-1 h-8">
        {[1, 2, 3, 4, 5].map((i) => (
          <div
            key={i}
            className={`
              w-1 bg-blue-500 rounded-full transition-all duration-150
              ${isSpeaking ? 'animate-soundbar' : 'h-2'}
            `}
            style={{
              height: isSpeaking
                ? `${Math.random() * 100}%`
                : '8px',
              animationDelay: `${i * 50}ms`,
            }}
          />
        ))}
      </div>

      {/* Status text */}
      <span className="text-sm text-gray-400">
        {isSpeaking ? 'Listening...' : 'Ready'}
        {probability !== undefined && (
          <span className="ml-2 text-xs opacity-60">
            ({Math.round(probability * 100)}%)
          </span>
        )}
      </span>
    </div>
  );
}
