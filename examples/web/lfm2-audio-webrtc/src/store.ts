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

/// Pipeline loading state, driven by `__system__.out` events from the server.
export type PipelineStatus =
  | { kind: 'unknown' }
  | { kind: 'initializing'; message?: string }
  | { kind: 'loading_node'; node?: string; message?: string }
  | { kind: 'ready'; message?: string }

/// One node's stats inside a `__perf__` snapshot. Mirrors the Rust
/// `PerfNodeStats` shape — keep in sync with crates/core/src/data/perf.rs.
export interface PerfLatencyPercentiles {
  p50_us: number
  p95_us: number
  p99_us: number
  max_us: number
}

export interface PerfNodeStats {
  inputs: number
  outputs: number
  latency_us: PerfLatencyPercentiles
  first_output_latency_us: PerfLatencyPercentiles
}

/// One periodic snapshot from the server's perf aggregator. Published
/// every `window_ms` (default 1 s) on `__perf__.out` when
/// `REMOTEMEDIA_PERF_TAP=1` is set on the server.
export interface PerfSnapshot {
  kind: 'perf_snapshot'
  session_id: string
  ts_ms: number
  window_ms: number
  nodes: Record<string, PerfNodeStats>
}

/// Per-node sticky stats kept across snapshots. The aggregator on the
/// server resets its histograms every window, so a node that's idle
/// for a tick disappears from the next snapshot's `nodes` map. The
/// HUD wants "what were this node's last meaningful numbers" and
/// "how long since we last saw activity" — that's what this row
/// provides.
export interface PerfNodeRow {
  stats: PerfNodeStats
  /** ts_ms of the snapshot these stats came from. */
  lastActivityMs: number
  /** Most recent snapshot ts_ms — used to compute "ago" age. */
  lastSeenMs: number
}

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

  /** Pipeline loading state from `__system__.out` events. */
  pipelineStatus: PipelineStatus

  /** Latest perf snapshot from `__perf__.out`. `null` when the server
   *  has perf tap disabled or hasn't emitted a snapshot yet. */
  perfSnapshot: PerfSnapshot | null

  /** Sticky per-node performance rows. A row is created on first
   *  activity and persists across idle windows; `lastActivityMs`
   *  tracks when its current stats were last refreshed. The HUD
   *  uses this so kokoro_tts (which only runs once per turn) keeps
   *  showing its last measurement instead of disappearing during
   *  the next idle second. */
  perfNodes: Record<string, PerfNodeRow>

  setStatus: (s: ConnectionStatus) => void
  setError: (e: string | null) => void
  setPeerId: (p: string | null) => void
  setRemoteAudioStream: (s: MediaStream | null) => void
  setLocalStream: (s: MediaStream | null) => void
  setPipelineStatus: (s: PipelineStatus) => void
  setPerfSnapshot: (s: PerfSnapshot | null) => void
  /** Merge a new snapshot into the sticky `perfNodes` map. Active
   *  nodes overwrite their row; inactive ones bump `lastSeenMs`
   *  but keep their previous stats. */
  ingestPerfSnapshot: (s: PerfSnapshot) => void
  clearPerfNodes: () => void

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
  pipelineStatus: { kind: 'unknown' } as PipelineStatus,
  perfSnapshot: null,
  perfNodes: {},

  setStatus: (status) => set({ status }),
  setError: (error) => set({ error }),
  setPeerId: (peerId) => set({ peerId }),
  setRemoteAudioStream: (remoteAudioStream) => set({ remoteAudioStream }),
  setLocalStream: (localStream) => set({ localStream }),
  setPipelineStatus: (pipelineStatus) => set({ pipelineStatus }),
  setPerfSnapshot: (perfSnapshot) => set({ perfSnapshot }),

  ingestPerfSnapshot: (snap) => {
    const prev = get().perfNodes
    const next: Record<string, PerfNodeRow> = { ...prev }

    // 1. Update rows for every node that had activity in this snapshot.
    for (const [id, stats] of Object.entries(snap.nodes)) {
      const active = stats.inputs > 0 || stats.outputs > 0
      if (active) {
        next[id] = {
          stats,
          lastActivityMs: snap.ts_ms,
          lastSeenMs: snap.ts_ms,
        }
      } else if (prev[id]) {
        // Node still in snapshot but idle this window — keep last
        // stats, bump lastSeenMs so "ago" updates.
        next[id] = { ...prev[id], lastSeenMs: snap.ts_ms }
      }
      // else: zero stats AND no prior row — ignore (no signal yet).
    }

    // 2. Bump lastSeenMs on rows we already had but that fell out
    //    of the snapshot map entirely. Their stats stay.
    for (const id of Object.keys(prev)) {
      if (!(id in snap.nodes)) {
        next[id] = { ...prev[id], lastSeenMs: snap.ts_ms }
      }
    }

    set({ perfSnapshot: snap, perfNodes: next })
  },

  clearPerfNodes: () => set({ perfNodes: {}, perfSnapshot: null }),

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
      pipelineStatus: { kind: 'unknown' },
      perfSnapshot: null,
      perfNodes: {},
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
