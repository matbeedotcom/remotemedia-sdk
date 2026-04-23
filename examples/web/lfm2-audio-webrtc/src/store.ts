// UI state for the LFM2-Audio observer.
//
// The store holds transient conversation state (live transcripts, VAD
// speech flag, per-turn history, knowledge-inject log) plus connection
// status. Persistent session state (chat history, context) lives on the
// server inside LFM2AudioNode — we only mirror what we receive.

import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'

export type ConnectionStatus =
  | 'idle'
  | 'connecting'
  | 'connected'
  | 'failed'
  | 'disconnected'

export interface VADSnapshot {
  hasSpeech: boolean
  probability: number
  isSpeechStart: boolean
  isSpeechEnd: boolean
  ts: number
  /** RMS of the mono resampled audio the VAD just saw (0..~1). */
  rms: number
  /** Peak absolute sample of the same chunk. */
  peak: number
  /** Chunk length in samples (post-resample). */
  samples: number
  /** Sample rate of the chunk (VAD always wants 16 kHz mono). */
  sampleRate: number
}

export interface Turn {
  id: number
  /** Relative start (ms since app load). */
  startedAt: number
  endedAt: number | null
  /** Transcript of the user's utterance (from stt_in). */
  userTranscript: string
  /** Transcript of the assistant reply (from stt_out). */
  assistantTranscript: string
  /** Assistant text tokens streamed live from audio.out — "live caption". */
  liveReply: string
  /** Whether LFM2 is actively generating. */
  generating: boolean
  /** True if this turn was barge-in'd mid-generation. */
  bargedIn: boolean
}

export interface KnowledgeEntry {
  id: number
  text: string
  at: number
}

export interface SettingsState {
  /** WebSocket URL for the remotemedia signaling server. */
  wsUrl: string
  /** Whether to auto-publish `audio.in.barge_in` on vad speech-start
   *  while the assistant is generating. */
  autoBargeIn: boolean
  /** Pending knowledge-injection text, editable in the KnowledgePane.
   *  Persisted in sessionStorage so a page reload keeps it, and
   *  auto-applied on connect. */
  draftKnowledge: string
  /** Pending system-prompt text. Same lifecycle as `draftKnowledge`. */
  draftSystemPrompt: string
}

interface StoreState extends SettingsState {
  status: ConnectionStatus
  error: string | null
  peerId: string | null
  remoteAudioStream: MediaStream | null
  /** Local mic MediaStream, exposed for the live meter component. */
  localStream: MediaStream | null

  vad: VADSnapshot | null
  turns: Turn[]
  currentTurnId: number | null
  knowledge: KnowledgeEntry[]

  setStatus: (s: ConnectionStatus) => void
  setError: (e: string | null) => void
  setPeerId: (p: string | null) => void
  setRemoteAudioStream: (s: MediaStream | null) => void
  setLocalStream: (s: MediaStream | null) => void

  setVad: (v: VADSnapshot) => void

  /** Begin a new turn on speech-start. Idempotent if a turn is already
   *  open (repeated is_speech_start events). */
  beginTurn: () => void
  /** Mark the user portion ended (we have the utterance). Also fires
   *  when stt_in arrives. */
  endUserTurn: () => void
  setUserTranscript: (text: string) => void
  setGenerating: (on: boolean) => void
  appendLiveReply: (chunk: string) => void
  setAssistantTranscript: (text: string) => void
  markBargedIn: () => void
  finalizeTurn: () => void

  addKnowledge: (text: string) => void
  clearKnowledge: () => void

  setSettings: (s: Partial<SettingsState>) => void
  reset: () => void
}

function defaultWsUrl(): string {
  // If the SPA is served from the same origin as the server, co-locate.
  // Otherwise fall back to localhost on the server's default port.
  const override = new URLSearchParams(window.location.search).get('ws')
  if (override) return override
  return 'ws://127.0.0.1:8081/ws'
}

let nextTurnId = 1
let nextKnowledgeId = 1

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const __useStore = create<StoreState>()(
  persist(
    (set, get) => ({
  wsUrl: defaultWsUrl(),
  autoBargeIn: true,
  draftKnowledge: '',
  draftSystemPrompt: '',
  status: 'idle',
  error: null,
  peerId: null,
  remoteAudioStream: null,
  localStream: null,

  vad: null,
  turns: [],
  currentTurnId: null,
  knowledge: [],

  setStatus: (status) => set({ status }),
  setError: (error) => set({ error }),
  setPeerId: (peerId) => set({ peerId }),
  setRemoteAudioStream: (remoteAudioStream) => set({ remoteAudioStream }),
  setLocalStream: (localStream) => set({ localStream }),

  setVad: (vad) => set({ vad }),

  beginTurn: () => {
    const { currentTurnId, turns } = get()
    if (currentTurnId !== null) {
      const t = turns.find((x) => x.id === currentTurnId)
      if (t && t.endedAt === null) return // still open
    }
    const id = nextTurnId++
    const t: Turn = {
      id,
      startedAt: performance.now(),
      endedAt: null,
      userTranscript: '',
      assistantTranscript: '',
      liveReply: '',
      generating: false,
      bargedIn: false,
    }
    set({
      currentTurnId: id,
      turns: [...turns, t],
    })
  },

  endUserTurn: () => {
    // no-op placeholder — user transcript arrival is the real signal.
  },

  setUserTranscript: (text) => {
    const { turns, currentTurnId } = get()
    if (currentTurnId === null) return
    set({
      turns: turns.map((t) =>
        t.id === currentTurnId ? { ...t, userTranscript: text } : t,
      ),
    })
  },

  setGenerating: (on) => {
    const { turns, currentTurnId } = get()
    if (currentTurnId === null) return
    set({
      turns: turns.map((t) =>
        t.id === currentTurnId ? { ...t, generating: on } : t,
      ),
    })
  },

  appendLiveReply: (chunk) => {
    const { turns, currentTurnId } = get()
    if (currentTurnId === null) return
    set({
      turns: turns.map((t) =>
        t.id === currentTurnId
          ? { ...t, liveReply: t.liveReply + chunk, generating: true }
          : t,
      ),
    })
  },

  setAssistantTranscript: (text) => {
    const { turns, currentTurnId } = get()
    if (currentTurnId === null) return
    set({
      turns: turns.map((t) =>
        t.id === currentTurnId ? { ...t, assistantTranscript: text } : t,
      ),
    })
  },

  markBargedIn: () => {
    const { turns, currentTurnId } = get()
    if (currentTurnId === null) return
    set({
      turns: turns.map((t) =>
        t.id === currentTurnId ? { ...t, bargedIn: true, generating: false } : t,
      ),
    })
  },

  finalizeTurn: () => {
    const { turns, currentTurnId } = get()
    if (currentTurnId === null) return
    set({
      currentTurnId: null,
      turns: turns.map((t) =>
        t.id === currentTurnId
          ? { ...t, endedAt: performance.now(), generating: false }
          : t,
      ),
    })
  },

  addKnowledge: (text) => {
    const k: KnowledgeEntry = {
      id: nextKnowledgeId++,
      text,
      at: performance.now(),
    }
    set({ knowledge: [...get().knowledge, k] })
  },

  clearKnowledge: () => set({ knowledge: [] }),

  setSettings: (s) => set(s),

  reset: () => {
    set({
      vad: null,
      turns: [],
      currentTurnId: null,
      knowledge: [],
      error: null,
    })
  },
    }),
    {
      name: 'lfm2-audio-webrtc-settings',
      // Use sessionStorage so tabs have their own prefilled values but
      // the same tab survives reloads. Swap to localStorage if you
      // want prefills to persist across tabs/sessions.
      storage: createJSONStorage(() => sessionStorage),
      // Only whitelist the "user-facing settings" fields. Transient
      // runtime state (turns, vad snapshots, remoteAudioStream, etc.)
      // must never be persisted — it would rehydrate stale values
      // into a fresh session.
      partialize: (state) => ({
        wsUrl: state.wsUrl,
        autoBargeIn: state.autoBargeIn,
        draftKnowledge: state.draftKnowledge,
        draftSystemPrompt: state.draftSystemPrompt,
      }),
    },
  ),
)

export const useStore = __useStore

// Test-only bridge: Playwright sets `window.__TEST__ = true` in an
// init script so specs can observe store state without scraping the
// DOM. No effect in production.
if (
  typeof window !== 'undefined' &&
  (window as unknown as { __TEST__?: boolean }).__TEST__
) {
  ;(window as unknown as { __store: typeof __useStore }).__store = __useStore
}
