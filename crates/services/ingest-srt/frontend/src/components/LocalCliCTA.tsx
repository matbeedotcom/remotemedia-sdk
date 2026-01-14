import { useState } from 'react';

/**
 * Compact CTA shown inside the observer UI for users who want local-only execution.
 *
 * The observer UI is designed to connect to a service session. This CTA points users
 * to the desktop demo CLI (`remotemedia-demo`) which emits the same JSONL events
 * without sending media to a remote service.
 */
export function LocalCliCTA() {
  const [copied, setCopied] = useState(false);

  // Keep this command intentionally minimal + generic so it works across environments.
  const command = `cd examples/cli/stream-health-demo
cargo build --release
./target/release/remotemedia-demo --ingest udp://127.0.0.1:5004 --json -q`;

  const copy = () => {
    navigator.clipboard.writeText(command);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="rounded-xl border border-surface-elevated bg-surface-secondary p-4 space-y-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-sm font-medium text-text-primary">Prefer local-only?</p>
          <p className="text-xs text-text-muted mt-1">
            Run the desktop CLI and get the same event stream (JSONL) without streaming media to this service.
          </p>
        </div>
        <button
          onClick={copy}
          className="flex-shrink-0 px-3 py-1.5 text-xs rounded bg-surface-elevated hover:bg-surface-card transition-colors text-text-secondary"
        >
          {copied ? 'Copied!' : 'Copy'}
        </button>
      </div>

      <pre className="p-3 bg-surface-primary rounded-lg text-xs font-mono overflow-x-auto whitespace-pre-wrap break-all text-text-secondary">
        {command}
      </pre>

      <p className="text-[11px] text-text-muted">
        Tip: feed your live pipeline a side-car copy (e.g. FFmpeg tee to UDP), then point the CLI at that local UDP URL.
      </p>
    </div>
  );
}

