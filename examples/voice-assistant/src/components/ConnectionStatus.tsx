import { Wifi, WifiOff, Loader2 } from 'lucide-react';

interface ConnectionStatusProps {
  mode: string;
  isConnected?: boolean;
  isConnecting?: boolean;
}

export function ConnectionStatus({ mode, isConnected = true, isConnecting = false }: ConnectionStatusProps) {
  // Local mode doesn't need connection status
  if (mode === 'local') {
    return null;
  }

  return (
    <div className="flex items-center gap-2">
      {isConnecting ? (
        <>
          <Loader2 className="w-4 h-4 text-yellow-500 animate-spin" />
          <span className="text-sm text-yellow-500">Connecting...</span>
        </>
      ) : isConnected ? (
        <>
          <Wifi className="w-4 h-4 text-green-500" />
          <span className="text-sm text-green-500">Connected</span>
        </>
      ) : (
        <>
          <WifiOff className="w-4 h-4 text-red-500" />
          <span className="text-sm text-red-500">Disconnected</span>
        </>
      )}
    </div>
  );
}
