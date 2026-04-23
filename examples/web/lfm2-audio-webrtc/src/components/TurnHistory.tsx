import { useStore } from '../store'

export function TurnHistory() {
  const turns = useStore((s) => s.turns)
  const currentTurnId = useStore((s) => s.currentTurnId)
  // Show previous, finalized turns only — current turn has its own pane.
  const past = turns.filter(
    (t) => t.id !== currentTurnId && t.endedAt !== null,
  )

  return (
    <div className="panel p-4 flex flex-col gap-3 min-h-0 flex-1">
      <div className="text-xs uppercase tracking-wider text-slate-400">
        history ({past.length})
      </div>
      {past.length === 0 ? (
        <div className="text-sm text-slate-500 italic">
          Completed turns will appear here with user + assistant
          transcripts and any barge-ins that happened mid-turn.
        </div>
      ) : (
        <ol className="flex flex-col gap-3 overflow-auto">
          {past
            .slice()
            .reverse()
            .map((t) => (
              <li
                key={t.id}
                className="border border-slate-800 rounded-md p-3 bg-slate-950/40"
              >
                <div className="flex items-center justify-between text-xs text-slate-500 mb-2">
                  <span>turn #{t.id}</span>
                  <span>
                    {((t.endedAt! - t.startedAt) / 1000).toFixed(1)}s
                    {t.bargedIn && (
                      <span className="ml-2 pill-warn">barged-in</span>
                    )}
                  </span>
                </div>
                <div className="flex items-start mb-1">
                  <span className="tag mt-0.5">you</span>
                  <p className="transcript-line text-slate-200 flex-1 whitespace-pre-wrap">
                    {t.userTranscript || (
                      <span className="text-slate-500 italic">
                        (no transcript)
                      </span>
                    )}
                  </p>
                </div>
                <div className="flex items-start">
                  <span className="tag mt-0.5">bot</span>
                  <p className="transcript-line text-slate-300 flex-1 whitespace-pre-wrap">
                    {t.assistantTranscript || t.liveReply || (
                      <span className="text-slate-500 italic">
                        (no reply)
                      </span>
                    )}
                  </p>
                </div>
              </li>
            ))}
        </ol>
      )}
    </div>
  )
}
