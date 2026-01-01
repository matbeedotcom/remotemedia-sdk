import { useSessionStore } from '@/store/session';
import { useSSE } from '@/hooks/useSSE';
import { useMetrics } from '@/hooks/useMetrics';
import { StatusHeader } from '@/components/StatusHeader';
import { Timeline } from '@/components/Timeline';
import { EvidencePane } from '@/components/EvidencePane';
import { SessionSetup } from '@/components/SessionSetup';
import { HealthScore } from '@/components/HealthScore';

export default function App() {
  const session = useSessionStore((s) => s.session);
  const status = useSessionStore((s) => s.status);

  // Connect SSE when session exists
  useSSE(session?.session_id ?? null);

  // Poll metrics
  useMetrics();

  const isActive = status !== 'idle';

  return (
    <div className="min-h-screen bg-surface-primary flex flex-col">
      {/* Status header (fixed) */}
      <StatusHeader />

      {/* Main content */}
      <main className="flex-1 flex pt-14">
        {!isActive ? (
          // Centered setup when idle
          <div className="flex-1 flex items-center justify-center">
            <div className="w-full max-w-md bg-surface-card rounded-xl border border-surface-elevated shadow-lg">
              <SessionSetup />
            </div>
          </div>
        ) : (
          // Three-column layout when active
          <div className="flex-1 flex">
            {/* Left: Session controls */}
            <aside className="w-80 border-r border-surface-elevated flex flex-col bg-surface-secondary">
              <HealthScore />
              <SessionSetup />
            </aside>

            {/* Center: Timeline */}
            <div className="flex-1 flex flex-col bg-surface-primary">
              <div className="px-4 py-3 border-b border-surface-elevated">
                <h2 className="text-sm font-medium text-text-secondary">Timeline</h2>
              </div>
              <Timeline />
            </div>

            {/* Right: Evidence pane */}
            <aside className="w-96 border-l border-surface-elevated bg-surface-secondary">
              <EvidencePane />
            </aside>
          </div>
        )}
      </main>
    </div>
  );
}
