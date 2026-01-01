import { useState } from 'react';
import { useSessionStore } from '@/store/session';
import { PIPELINES, type Session, type CreateSessionRequest } from '@/types/session';
import clsx from 'clsx';

const API_BASE = '';

export function SessionSetup() {
  const session = useSessionStore((s) => s.session);
  const setSession = useSessionStore((s) => s.setSession);
  const reset = useSessionStore((s) => s.reset);

  const [pipeline, setPipeline] = useState('contact_center_qa_v1');
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const createSession = async () => {
    setIsCreating(true);
    setError(null);

    try {
      const request: CreateSessionRequest = {
        pipeline,
        audio_enabled: true,
        video_enabled: false,
        max_duration_seconds: 300,
      };

      const response = await fetch(`${API_BASE}/api/ingest/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(request),
      });

      if (!response.ok) {
        const err = await response.json();
        throw new Error(err.error || 'Failed to create session');
      }

      const sessionData: Session = await response.json();
      setSession(sessionData);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setIsCreating(false);
    }
  };

  const endSession = async () => {
    if (!session) return;

    try {
      await fetch(`${API_BASE}/api/ingest/sessions/${session.session_id}`, {
        method: 'DELETE',
      });
    } catch (err) {
      console.error('Failed to end session:', err);
    }

    reset();
  };

  const copyCommand = () => {
    if (!session) return;
    navigator.clipboard.writeText(session.ffmpeg_command_transcode);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  // Show active session
  if (session) {
    return (
      <div className="p-6 space-y-6">
        <div className="space-y-1">
          <h3 className="text-sm font-medium text-status-ok">Session Active</h3>
          <p className="text-xs text-text-muted font-mono">{session.session_id}</p>
        </div>

        <div className="space-y-3">
          <p className="text-sm text-text-secondary">
            Run this command to start streaming:
          </p>
          <div className="relative">
            <pre className="p-4 bg-surface-primary rounded-lg text-xs font-mono text-text-secondary overflow-x-auto whitespace-pre-wrap break-all">
              {session.ffmpeg_command_transcode}
            </pre>
            <button
              onClick={copyCommand}
              className="absolute top-2 right-2 px-3 py-1.5 text-xs rounded bg-surface-elevated hover:bg-surface-card transition-colors"
            >
              {copied ? 'Copied!' : 'Copy'}
            </button>
          </div>
          <p className="text-xs text-text-muted">
            This does not modify your stream. It only observes a copy.
          </p>
        </div>

        <button
          onClick={endSession}
          className="w-full py-2.5 rounded-lg bg-status-error/20 text-status-error hover:bg-status-error/30 transition-colors text-sm font-medium"
        >
          End Session
        </button>
      </div>
    );
  }

  // Show setup form
  return (
    <div className="p-6 space-y-6">
      <div className="text-center space-y-2">
        <h2 className="text-xl font-semibold text-text-primary">Waiting for media...</h2>
        <p className="text-sm text-text-secondary">
          Select a pipeline and create a session to start
        </p>
      </div>

      <div className="space-y-4">
        <div>
          <label className="block text-sm text-text-secondary mb-2">Pipeline</label>
          <div className="space-y-2">
            {(['business', 'technical', 'audio', 'video'] as const).map((category) => {
              const categoryPipelines = PIPELINES.filter((p) => p.category === category);
              if (categoryPipelines.length === 0) return null;

              return (
                <div key={category}>
                  <p className="text-xs text-text-muted uppercase tracking-wider mb-1.5">
                    {category}
                  </p>
                  <div className="space-y-1">
                    {categoryPipelines.map((p) => (
                      <button
                        key={p.id}
                        onClick={() => setPipeline(p.id)}
                        className={clsx(
                          'w-full text-left px-3 py-2 rounded-lg transition-all',
                          'border',
                          pipeline === p.id
                            ? 'bg-surface-elevated border-status-info/50'
                            : 'bg-surface-secondary border-transparent hover:border-surface-elevated'
                        )}
                      >
                        <p className="text-sm font-medium text-text-primary">{p.name}</p>
                        <p className="text-xs text-text-muted">{p.description}</p>
                      </button>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {error && (
          <div className="p-3 rounded-lg bg-status-error/10 border border-status-error/30 text-status-error text-sm">
            {error}
          </div>
        )}

        <button
          onClick={createSession}
          disabled={isCreating}
          className={clsx(
            'w-full py-3 rounded-lg font-medium text-sm transition-all',
            'bg-status-info text-white hover:bg-status-info/90',
            'disabled:opacity-50 disabled:cursor-not-allowed'
          )}
        >
          {isCreating ? 'Creating...' : 'Create Session'}
        </button>
      </div>
    </div>
  );
}
