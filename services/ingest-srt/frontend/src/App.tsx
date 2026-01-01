import { useSessionStore } from '@/store/session';
import { useSSE } from '@/hooks/useSSE';
import { useMetrics } from '@/hooks/useMetrics';
import { useSessionCreation } from '@/hooks/useSessionCreation';
import { StatusHeader } from '@/components/StatusHeader';
import { Timeline } from '@/components/Timeline';
import { EvidencePane } from '@/components/EvidencePane';
import { SessionSetup } from '@/components/SessionSetup';
import { SessionSummary } from '@/components/SessionSummary';
import { IngestCommand } from '@/components/IngestCommand';
import { HealthScore } from '@/components/HealthScore';

export default function App() {
  const session = useSessionStore((s) => s.session);
  const status = useSessionStore((s) => s.status);
  const selectedPipeline = useSessionStore((s) => s.selectedPipeline);
  const commandLocked = useSessionStore((s) => s.commandLocked);

  // Connect SSE when session exists
  useSSE(session?.session_id ?? null);

  // Poll metrics
  useMetrics();

  // Auto-create session when pipeline selected (must be in App to stay mounted)
  const { isCreating, error: sessionError } = useSessionCreation();

  // Determine what state we're in for layout purposes
  const isIdle = status === 'idle' && !selectedPipeline;
  const isReady = (status === 'idle' || status === 'created' || status === 'connecting') && selectedPipeline;
  const isStreaming = status === 'streaming';
  const isDisconnected = status === 'disconnected';
  const showPanels = !isIdle; // Show side panels once pipeline selected

  return (
    <div className="min-h-screen bg-surface-primary flex flex-col">
      {/* Status header (fixed) */}
      <StatusHeader />

      {/* Main content - always three-column layout */}
      <main className="flex-1 flex pt-14">
        {/* Left panel: Controls/Ingest */}
        <aside className={`w-80 border-r border-surface-elevated flex flex-col bg-surface-secondary ${
          isIdle ? 'opacity-0' : 'animate-slide-in'
        }`}>
          {showPanels && (
            <div className="p-6 space-y-6">
              {(isStreaming || isDisconnected) && <HealthScore />}
              {sessionError && (
                <div className="p-3 rounded-lg bg-status-error/10 border border-status-error/30 text-status-error text-sm">
                  {sessionError}
                </div>
              )}
              {session ? (
                <IngestCommand
                  command={session.ffmpeg_command}
                  isLocked={commandLocked}
                />
              ) : isCreating ? (
                <div className="text-sm text-text-muted">
                  Creating session...
                </div>
              ) : null}
            </div>
          )}
        </aside>

        {/* Center panel: Timeline or Landing */}
        <div className="flex-1 flex flex-col bg-surface-primary">
          {isIdle ? (
            // Landing state - centered pipeline selection
            <div className="flex-1 flex items-center justify-center">
              <div className="text-center animate-fade-in">
                <h1 className="text-2xl font-light text-text-primary mb-2">
                  Waiting for media...
                </h1>
                <p className="text-sm text-text-muted mb-8">
                  Choose what you want to observe.
                </p>
                <div className="w-full max-w-2xl mx-auto">
                  <SessionSetup />
                </div>
              </div>
            </div>
          ) : isDisconnected ? (
            // Disconnected state - show session summary
            <SessionSummary />
          ) : isReady ? (
            // Ready state - waiting for stream to connect
            <div className="flex-1 flex flex-col animate-fade-in">
              <div className="px-4 py-3 border-b border-surface-elevated">
                <h2 className="text-sm font-medium text-text-secondary">Timeline</h2>
              </div>
              <div className="flex-1 flex items-center justify-center">
                <p className="text-sm text-text-muted">
                  Run the FFmpeg command to start streaming
                </p>
              </div>
            </div>
          ) : (
            // Streaming state - show timeline with events
            <div className="flex-1 flex flex-col animate-fade-in">
              <div className="px-4 py-3 border-b border-surface-elevated">
                <h2 className="text-sm font-medium text-text-secondary">Timeline</h2>
              </div>
              <Timeline />
            </div>
          )}
        </div>

        {/* Right panel: Evidence */}
        <aside className={`w-96 border-l border-surface-elevated bg-surface-secondary ${
          isIdle ? 'opacity-0' : 'animate-fade-in'
        }`}>
          {showPanels && <EvidencePane />}
        </aside>
      </main>
    </div>
  );
}
