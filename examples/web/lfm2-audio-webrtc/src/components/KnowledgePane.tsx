import { useState } from 'react'
import { useStore } from '../store'

interface Props {
  onInject: (text: string) => Promise<void>
  onSystemPrompt: (text: string) => Promise<void>
}

export function KnowledgePane({ onInject, onSystemPrompt }: Props) {
  const knowledge = useStore((s) => s.knowledge)
  const clearKnowledge = useStore((s) => s.clearKnowledge)
  const status = useStore((s) => s.status)
  // Bind the textareas to the persisted drafts so values survive
  // reloads and are auto-applied on connect (see session.start()).
  const text = useStore((s) => s.draftKnowledge)
  const persona = useStore((s) => s.draftSystemPrompt)
  const setSettings = useStore((s) => s.setSettings)
  const setText = (v: string) => setSettings({ draftKnowledge: v })
  const setPersona = (v: string) => setSettings({ draftSystemPrompt: v })
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  const connected = status === 'connected'

  async function submit() {
    if (!text.trim() || !connected || busy) return
    setBusy(true)
    setErr(null)
    try {
      await onInject(text.trim())
      // Keep the textarea populated for reconnects — don't clear it.
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  async function savePersona() {
    if (!persona.trim() || !connected || busy) return
    setBusy(true)
    setErr(null)
    try {
      await onSystemPrompt(persona.trim())
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="panel p-4 flex flex-col gap-3 min-h-0 flex-1">
      <div className="text-xs uppercase tracking-wider text-slate-400">
        knowledge injection
      </div>
      <p className="text-xs text-slate-500 leading-relaxed">
        Text you send here is published to{' '}
        <code className="font-mono text-slate-300">audio.in.context</code>.
        The server prepends it to the system turn on the next
        generation; it persists until you reset.
      </p>
      <textarea
        className="bg-slate-950 border border-slate-800 rounded-md p-2 text-sm leading-snug text-slate-100 resize-y min-h-[5rem]"
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="e.g. The user's name is Mathieu. Today's meetings are at 10am, 2pm, 4pm."
        onKeyDown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') submit()
        }}
      />
      <div className="flex items-center gap-2">
        <button
          className="btn-primary"
          onClick={submit}
          disabled={!connected || busy || !text.trim()}
        >
          inject
        </button>
        <span className="text-xs text-slate-500">⌘/ctrl-enter to send</span>
        <div className="flex-1" />
        {knowledge.length > 0 && (
          <button
            className="btn-ghost text-xs"
            onClick={clearKnowledge}
            title="Clear local log (does NOT wipe server context — use reset for that)"
          >
            clear log
          </button>
        )}
      </div>

      <div className="mt-2 border-t border-slate-800 pt-3">
        <div className="text-xs uppercase tracking-wider text-slate-400 mb-1">
          persona (system prompt)
        </div>
        <textarea
          className="bg-slate-950 border border-slate-800 rounded-md p-2 text-sm leading-snug text-slate-100 resize-y min-h-[3.5rem]"
          value={persona}
          onChange={(e) => setPersona(e.target.value)}
          placeholder="You are a concise voice assistant…"
        />
        <button
          className="btn-ghost text-xs mt-2"
          onClick={savePersona}
          disabled={!connected || busy || !persona.trim()}
        >
          set persona
        </button>
      </div>

      {err && (
        <div className="text-xs text-red-400 mt-1" role="alert">
          {err}
        </div>
      )}

      <div className="mt-2 border-t border-slate-800 pt-3 flex-1 min-h-0 flex flex-col">
        <div className="text-xs uppercase tracking-wider text-slate-400 mb-2">
          injected ({knowledge.length})
        </div>
        {knowledge.length === 0 ? (
          <div className="text-sm text-slate-500 italic">
            Nothing injected yet.
          </div>
        ) : (
          <ol className="flex flex-col gap-2 overflow-auto">
            {knowledge
              .slice()
              .reverse()
              .map((k) => (
                <li
                  key={k.id}
                  className="border border-slate-800 bg-slate-950/40 rounded-md p-2"
                >
                  <div className="text-[10px] text-slate-500 mb-1">
                    +{(k.at / 1000).toFixed(1)}s
                  </div>
                  <div className="text-sm text-slate-200 whitespace-pre-wrap">
                    {k.text}
                  </div>
                </li>
              ))}
          </ol>
        )}
      </div>
    </div>
  )
}
