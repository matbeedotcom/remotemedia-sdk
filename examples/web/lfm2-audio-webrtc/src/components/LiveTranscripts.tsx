import { useStore } from '../store'

export function LiveTranscripts() {
  const turns = useStore((s) => s.turns)
  const currentTurnId = useStore((s) => s.currentTurnId)
  const current = currentTurnId
    ? turns.find((t) => t.id === currentTurnId)
    : null

  return (
    <div className="panel p-4 flex flex-col gap-3 min-h-0">
      <div className="flex items-center justify-between">
        <div className="text-xs uppercase tracking-wider text-slate-400">
          current turn
        </div>
        {current && (
          <div className="text-xs text-slate-500">
            turn #{current.id}
            {current.bargedIn && (
              <span className="ml-2 pill-warn">barged-in</span>
            )}
          </div>
        )}
      </div>
      {!current ? (
        <div className="text-sm text-slate-500 italic">
          Waiting for you to speak. VAD will open a turn as soon as it
          detects speech.
        </div>
      ) : (
        <div className="flex flex-col gap-3 min-h-0 overflow-auto">
          <TranscriptRow
            tag="you"
            text={
              current.userTranscript ||
              (current.endedAt === null
                ? '…listening…'
                : '(no transcript)')
            }
            muted={!current.userTranscript}
          />
          <TranscriptRow
            tag="assistant (live)"
            text={
              current.liveReply ||
              (current.generating ? '…generating…' : '(pending)')
            }
            muted={!current.liveReply}
            monospace
          />
          {current.assistantTranscript && (
            <TranscriptRow
              tag="assistant (whisper)"
              text={current.assistantTranscript}
            />
          )}
        </div>
      )}
    </div>
  )
}

function TranscriptRow({
  tag,
  text,
  muted,
  monospace,
}: {
  tag: string
  text: string
  muted?: boolean
  monospace?: boolean
}) {
  return (
    <div className="flex items-start">
      <span className="tag mt-0.5">{tag}</span>
      <p
        className={[
          'transcript-line flex-1 whitespace-pre-wrap',
          muted ? 'text-slate-500 italic' : 'text-slate-100',
          monospace ? 'font-mono' : '',
        ].join(' ')}
      >
        {text}
      </p>
    </div>
  )
}
