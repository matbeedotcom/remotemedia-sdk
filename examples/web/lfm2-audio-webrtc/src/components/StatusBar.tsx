import clsx from 'clsx'
import { useStore, type PipelineStatus } from '../store'

function VoiceActivityIcon({ active, level }: { active: boolean; level: number }) {
  const clamped = Math.max(0, Math.min(1, level))
  // Map (active, level) to one of 5 discrete bucket classes so we can
  // style bar heights from CSS instead of inline styles.
  const bucket = active ? Math.min(4, Math.floor(clamped * 5)) : -1
  return (
    <span
      aria-hidden="true"
      className={clsx(
        'voice-activity',
        active && `voice-activity--b${bucket}`,
        !active && 'voice-activity--idle',
      )}
    >
      <span className="voice-activity__bar voice-activity__bar--1" />
      <span className="voice-activity__bar voice-activity__bar--2" />
      <span className="voice-activity__bar voice-activity__bar--3" />
      <span className="voice-activity__bar voice-activity__bar--4" />
      <span className="voice-activity__bar voice-activity__bar--5" />
    </span>
  )
}

export function StatusBar({
  onReset,
  onBarge,
}: {
  onReset: () => void
  onBarge: () => void
}) {
  const status = useStore((s) => s.status)
  const error = useStore((s) => s.error)
  const peerId = useStore((s) => s.peerId)
  const vad = useStore((s) => s.vad)
  const turns = useStore((s) => s.turns)
  const currentTurnId = useStore((s) => s.currentTurnId)
  const pipelineStatus = useStore((s) => s.pipelineStatus)

  const current = currentTurnId
    ? turns.find((t) => t.id === currentTurnId)
    : null
  const generating = !!current?.generating
  const speaking = !!vad?.hasSpeech

  const statusLabel = status === 'connected'
    ? 'live'
    : status === 'connecting'
    ? 'connecting'
    : status === 'failed'
    ? 'failed'
    : status === 'disconnected'
    ? 'offline'
    : 'idle'

  const statusPill =
    status === 'connected'
      ? 'pill-on'
      : status === 'connecting'
      ? 'pill-warn'
      : status === 'failed'
      ? 'pill-off bg-red-900/50 text-red-300 border-red-800'
      : 'pill-off'

  // Pipeline loading indicator
  const pipelineLabel = getPipelineLabel(pipelineStatus)
  const pipelinePill = getPipelinePill(pipelineStatus)

  return (
    <div className="flex items-center gap-3 px-6 py-2 bg-slate-900/40 border-b border-slate-800 text-xs">
      <span className={statusPill}>
        <span
          className={clsx(
            'inline-block w-1.5 h-1.5 rounded-full mr-1',
            status === 'connected' ? 'bg-good' : 'bg-slate-500',
          )}
        />
        {statusLabel}
      </span>
      {peerId && <span className="text-slate-500">peer: {peerId}</span>}
      {pipelineLabel && (
        <span className={pipelinePill} title={getPipelineTitle(pipelineStatus, pipelineLabel)}>
          {pipelineLabel}
        </span>
      )}
      <div className="flex-1" />
      <span
        className={clsx(
          speaking ? 'pill-on pulse-live' : 'pill-off',
          'select-none',
        )}
        title={vad ? `p=${vad.probability.toFixed(2)}` : 'no VAD yet'}
      >
        <VoiceActivityIcon active={speaking} level={vad?.probability ?? 0} />
        {speaking ? 'user speaking' : 'silence'}
      </span>
      <span className={generating ? 'pill-warn' : 'pill-off'}>
        {generating ? 'assistant generating' : 'assistant idle'}
      </span>
      <button
        className="btn-ghost text-xs"
        onClick={onBarge}
        disabled={!generating}
        title="Force interrupt the current generation"
      >
        barge
      </button>
      <button
        className="btn-ghost text-xs"
        onClick={onReset}
        title="Wipe server-side chat history + context"
      >
        reset
      </button>
      {error && (
        <span className="ml-2 text-red-400 truncate max-w-[40%]" title={error}>
          {error}
        </span>
      )}
    </div>
  )
}

function getPipelineLabel(ps: PipelineStatus): string | null {
  switch (ps.kind) {
    case 'initializing':
      return 'initializing'
    case 'loading_node':
      return ps.node ? `loading ${ps.node}` : 'loading'
    case 'ready':
      return 'ready'
    default:
      return null
  }
}

function getPipelineTitle(ps: PipelineStatus, label: string): string {
  if ('message' in ps && ps.message) return ps.message
  return label
}

function getPipelinePill(ps: PipelineStatus): string {
  switch (ps.kind) {
    case 'initializing':
    case 'loading_node':
      return 'pill-warn pulse-live'
    case 'ready':
      return 'pill-on'
    default:
      return 'pill-off'
  }
}
