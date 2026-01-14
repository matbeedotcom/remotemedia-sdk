import { useState } from 'react';
import clsx from 'clsx';

interface IngestCommandProps {
  command: string;
  isLocked: boolean;
}

export function IngestCommand({ command, isLocked }: IngestCommandProps) {
  const [copied, setCopied] = useState(false);

  const copyCommand = () => {
    navigator.clipboard.writeText(command);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="space-y-3 animate-slide-in">
      <div className="flex items-center justify-between">
        <p className="text-sm text-text-secondary">
          {isLocked ? 'Stream connected' : 'Run this command to start streaming:'}
        </p>
        {isLocked && (
          <div className="flex items-center gap-1.5 text-status-ok">
            <svg
              className="w-4 h-4"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M5 13l4 4L19 7"
              />
            </svg>
            <span className="text-xs font-medium">Connected</span>
          </div>
        )}
      </div>

      <div className="relative">
        <pre
          className={clsx(
            'p-4 bg-surface-primary rounded-lg text-xs font-mono overflow-x-auto whitespace-pre-wrap break-all',
            isLocked ? 'text-text-muted' : 'text-text-secondary'
          )}
        >
          {command}
        </pre>
        {!isLocked && (
          <button
            onClick={copyCommand}
            className="absolute top-2 right-2 px-3 py-1.5 text-xs rounded bg-surface-elevated hover:bg-surface-card transition-colors text-text-secondary"
          >
            {copied ? 'Copied!' : 'Copy'}
          </button>
        )}
      </div>

      <p className="text-xs text-text-muted">
        This does not modify your stream. It only observes a copy.
      </p>
    </div>
  );
}
