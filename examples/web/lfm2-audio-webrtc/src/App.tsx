import { useRef, useState } from 'react'
import clsx from 'clsx'
import { useStore } from './store'
import { Session } from './session'
import { LiveTranscripts } from './components/LiveTranscripts'
import { KnowledgePane } from './components/KnowledgePane'
import { TurnHistory } from './components/TurnHistory'
import { StatusBar } from './components/StatusBar'
import { RemoteAudio } from './components/RemoteAudio'
import { AudioStats } from './components/AudioStats'
import { PerfHud } from './components/PerfHud'

function randomPeerId(): string {
  const rand =
    globalThis.crypto?.randomUUID?.() ??
    Math.random().toString(36).slice(2) + Date.now().toString(36)
  return `browser-${rand.slice(0, 8)}`
}

export default function App() {
  const status = useStore((s) => s.status)
  const wsUrl = useStore((s) => s.wsUrl)
  const autoBargeIn = useStore((s) => s.autoBargeIn)
  const setSettings = useStore((s) => s.setSettings)
  const sessionRef = useRef<Session | null>(null)
  const [starting, setStarting] = useState(false)

  async function start() {
    if (sessionRef.current || starting) return
    setStarting(true)
    try {
      const session = new Session(wsUrl, randomPeerId())
      sessionRef.current = session
      await session.start()
    } catch (e) {
      console.error(e)
      sessionRef.current?.stop()
      sessionRef.current = null
    } finally {
      setStarting(false)
    }
  }

  function stop() {
    sessionRef.current?.stop()
    sessionRef.current = null
  }

  async function sendKnowledge(text: string) {
    await sessionRef.current?.injectKnowledge(text)
  }
  async function sendSystemPrompt(text: string) {
    await sessionRef.current?.setSystemPrompt(text)
  }
  async function resetHistory() {
    await sessionRef.current?.resetHistory()
  }
  async function bargeNow() {
    await sessionRef.current?.bargeIn()
  }

  const isLive = status === 'connected'
  const isConnecting = status === 'connecting' || starting

  return (
    <div className="h-full flex flex-col">
      <header className="border-b border-slate-800 bg-slate-900/70 px-6 py-3 flex items-center gap-4">
        <div className="text-sm font-semibold tracking-wide text-slate-200">
          LFM2-Audio · WebRTC Observer
        </div>
        <div className="flex-1" />
        <label className="text-xs text-slate-400 flex items-center gap-2">
          <span>ws:</span>
          <input
            className="bg-slate-800 border border-slate-700 rounded px-2 py-1 text-slate-100 w-72"
            value={wsUrl}
            onChange={(e) => setSettings({ wsUrl: e.target.value })}
            disabled={isLive || isConnecting}
          />
        </label>
        <label className="text-xs text-slate-400 flex items-center gap-2">
          <input
            type="checkbox"
            checked={autoBargeIn}
            onChange={(e) => setSettings({ autoBargeIn: e.target.checked })}
          />
          auto barge-in
        </label>
        {isLive ? (
          <button className="btn-danger" onClick={stop}>
            stop
          </button>
        ) : (
          <button
            className={clsx('btn-primary', isConnecting && 'opacity-60')}
            onClick={start}
            disabled={isConnecting}
          >
            {isConnecting ? 'connecting…' : 'start mic'}
          </button>
        )}
      </header>

      <StatusBar onReset={resetHistory} onBarge={bargeNow} />

      <main className="flex-1 grid grid-cols-1 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)] gap-4 p-4 overflow-hidden">
        <section className="flex flex-col gap-4 min-h-0">
          <LiveTranscripts />
          <TurnHistory />
        </section>
        <aside className="flex flex-col gap-4 min-h-0">
          <KnowledgePane
            onInject={sendKnowledge}
            onSystemPrompt={sendSystemPrompt}
          />
          <PerfHud />
        </aside>
      </main>

      <RemoteAudio />
      <AudioStats />
    </div>
  )
}
