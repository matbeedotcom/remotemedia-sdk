// Session manager — wires SignalingClient + WebRtcPeer + ControlBusClient
// together, subscribes to the right control-bus topics, and feeds events
// into the Zustand store.
//
// Turn lifecycle is driven by the SERVER-AUTHORITATIVE
// `coordinator.out` stream published by the
// `ConversationCoordinatorNode` in the pipeline:
//
//   coordinator.out phase=USER_SPEAKING      -> beginTurn (new user turn)
//                                              + markBargedIn on previous
//                                                turn if cancelled_turn_id
//                                                is set
//   coordinator.out phase=AGENT_SPEAKING     -> setGenerating(true)
//   coordinator.out phase=IDLE               -> finalizeTurn
//
// The `vad.out` subscription is now used only to publish the existing
// barge-in aux-port fanout (`llm.in.barge_in`, `audio.in.barge_in`,
// `flush_audio`) and to populate the VAD UI indicators. A future
// iteration will move the aux-port fanout server-side into the
// coordinator and retire those publishes.
//
//   stt_in.out                -> set user transcript on the current turn
//   audio.out (kind=text)     -> append to liveReply
//
// Server-side audio flows over the WebRTC peer's remote track, not the
// control bus — the `audio.out` tap carries only the text-token stream
// from LFM2 (audio chunks are dropped server-side before serialization).

import { SignalingClient } from './signaling'
import { WebRtcPeer } from './webrtc'
import { ControlBusClient } from './control'
import { useStore } from './store'

export class Session {
  ws: SignalingClient
  peer: WebRtcPeer
  control: ControlBusClient
  private unsubscribers: Array<() => void> = []
  private localStream: MediaStream | null = null

  // Playback duration tracking — used to surface a "playing back"
  // indicator in the UI. Echo suppression itself is handled by the
  // browser's ``echoCancellation`` constraint on the mic track (see
  // ``getUserMedia`` below), not by muting the mic during replies —
  // muting would kill VAD and make voice barge-in impossible.
  private replyStartedAtMs: number | null = null
  private replyEstimatedDurationMs = 0
  private replyReleaseTimer: ReturnType<typeof setTimeout> | null = null

  constructor(wsUrl: string, peerId: string) {
    this.ws = new SignalingClient(wsUrl)
    this.peer = new WebRtcPeer(this.ws, peerId)
    this.control = new ControlBusClient(this.ws)
  }

  private setMicEnabled(enabled: boolean) {
    if (!this.localStream) return
    for (const track of this.localStream.getAudioTracks()) {
      // Only toggle when state actually changes so we don't kick
      // the capture device every chunk.
      if (track.enabled !== enabled) {
        track.enabled = enabled
      }
    }
  }

  async start(): Promise<void> {
    const store = useStore.getState()
    store.setStatus('connecting')
    store.setPeerId(this.peer.getPeerId())
    try {
      await this.ws.connect()
      this.ws.onClosed(() => {
        useStore.getState().setStatus('disconnected')
      })

      // Mic capture — mono to match the pipeline's expected channel layout.
      //
      // ``echoCancellation`` is ON by default: on laptops the
      // assistant's voice reflects off the room back into the mic,
      // the server VAD then fires and Whisper transcribes the
      // assistant's own reply as a new user turn ("echo loop"). AEC
      // uses the remote playback signal as a reference and subtracts
      // only the echoed component, so real user speech still carries
      // through loud enough for barge-in to work — the earlier worry
      // that AEC nukes barge-in is overstated for modern Chrome.
      //
      // ``noiseSuppression`` stays OFF: it's aggressive enough to gate
      // short/quiet user speech (single-word answers, quick "yes") and
      // the echo-loop problem is already handled by AEC.
      //
      // Users with external speakers who prefer the old no-AEC
      // behaviour (e.g. because their AEC implementation is poor) can
      // set ``window.__DISABLE_AEC = true`` before the session starts;
      // we read that override here.
      const disableAec = !!(
        typeof window !== 'undefined' &&
        (window as unknown as { __DISABLE_AEC?: boolean }).__DISABLE_AEC
      )
      this.localStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: !disableAec,
          noiseSuppression: false,
          autoGainControl: true,
        },
        video: false,
      })
      useStore.getState().setLocalStream(this.localStream)

      this.peer.setCallbacks({
        onRemoteTrack: (_track, streams) => {
          if (streams[0]) {
            useStore.getState().setRemoteAudioStream(streams[0])
          }
        },
        onStateChange: (s) => {
          if (s === 'connected') useStore.getState().setStatus('connected')
          else if (s === 'failed') {
            useStore.getState().setStatus('failed')
            useStore.getState().setError('WebRTC connection failed')
          } else if (s === 'disconnected' || s === 'closed') {
            useStore.getState().setStatus('disconnected')
          }
        },
      })

      await this.peer.connect(this.localStream)

      // Wire control-bus subscriptions. All topic names must match the
      // node IDs in the server manifest (see lfm2_audio_webrtc_server.rs).
      await this.wireSubscriptions()

      // Auto-apply persisted drafts so prefilled persona / knowledge
      // take effect from turn 1 — the user shouldn't have to re-click
      // "set persona" / "inject" after every reconnect.
      const { draftSystemPrompt, draftKnowledge } = useStore.getState()
      if (draftSystemPrompt.trim()) {
        try {
          await this.setSystemPrompt(draftSystemPrompt.trim())
        } catch (e) {
          console.warn('[session] failed to apply persisted system prompt:', e)
        }
      }
      if (draftKnowledge.trim()) {
        try {
          await this.injectKnowledge(draftKnowledge.trim())
        } catch (e) {
          console.warn('[session] failed to apply persisted knowledge:', e)
        }
      }
    } catch (e) {
      useStore.getState().setStatus('failed')
      useStore.getState().setError((e as Error).message)
      throw e
    }

    // Test-only bridge: Playwright uses window.__session to drive
    // the control bus from e2e specs without scraping the DOM.
    if (
      typeof window !== 'undefined' &&
      (window as unknown as { __TEST__?: boolean }).__TEST__
    ) {
      ;(window as unknown as { __session: Session }).__session = this
    }
  }

  private async wireSubscriptions(): Promise<void> {
    const { control } = this
    const s = () => useStore.getState()

    // Pipeline loading state from `__system__.out` events.
    // The server emits these during node initialization so the
    // frontend can show "initializing" vs "ready" indicators.
    this.unsubscribers.push(
      await control.subscribe('__system__.out', (ev) => {
        if (ev.kind !== 'json') return
        const p = ev.payload as {
          kind?: string
          status?: string
          node?: string
          message?: string
        }
        if (p.kind !== 'loading') return
        console.log('[system] loading:', p.status, p.message)
        switch (p.status) {
          case 'initializing':
            s().setPipelineStatus({ kind: 'initializing', message: p.message })
            break
          case 'loading_node':
            s().setPipelineStatus({
              kind: 'loading_node',
              node: p.node,
              message: p.message,
            })
            break
          case 'ready':
            s().setPipelineStatus({ kind: 'ready', message: p.message })
            break
          default:
            break
        }
      }),
    )

    // Periodic per-node performance snapshots from `__perf__.out`.
    // Server emits these when `REMOTEMEDIA_PERF_TAP=1`. Frontend
    // tolerates absence: if the env flag is off, no events arrive
    // and the HUD stays empty.
    //
    // We `ingestPerfSnapshot` rather than `setPerfSnapshot` so the
    // store's sticky-merge logic preserves stats for nodes that go
    // idle for a window (e.g. kokoro_tts only fires once per turn —
    // its histograms reset on the server each second, so a naive
    // "show latest snapshot" would make it disappear between turns).
    this.unsubscribers.push(
      await control.subscribe('__perf__.out', (ev) => {
        if (ev.kind !== 'json') return
        const p = ev.payload as Record<string, unknown>
        if (p?.kind !== 'perf_snapshot') return
        s().ingestPerfSnapshot(
          p as unknown as import('./store').PerfSnapshot,
        )
      }),
    )

    this.unsubscribers.push(
      await control.subscribe('vad.out', (ev) => {
        if (ev.kind !== 'json') return
        const p = ev.payload as {
          has_speech?: boolean
          speech_probability?: number
          is_speech_start?: boolean
          is_speech_end?: boolean
          timestamp_ms?: number
          rms?: number
          peak?: number
          samples?: number
          sample_rate?: number
        }
        s().setVad({
          hasSpeech: !!p.has_speech,
          probability: p.speech_probability ?? 0,
          isSpeechStart: !!p.is_speech_start,
          isSpeechEnd: !!p.is_speech_end,
          ts: ev.ts,
          rms: p.rms ?? 0,
          peak: p.peak ?? 0,
          samples: p.samples ?? 0,
          sampleRate: p.sample_rate ?? 16000,
        })
        if (p.is_speech_start) {
          // Barge-in is fully server-side now. The
          // `ConversationCoordinatorNode` observes this same VAD
          // event on its wired input and (a) publishes to
          // `llm.in.barge_in` + `audio.in.barge_in` via
          // SessionControl, and (b) pings a flush-audio hook
          // installed by the WebRTC `ServerPeer` which drains the
          // outbound ring buffer. No client round-trip required.
          if (this.replyReleaseTimer !== null) {
            clearTimeout(this.replyReleaseTimer)
            this.replyReleaseTimer = null
          }
          this.replyStartedAtMs = null
          this.replyEstimatedDurationMs = 0
          this.setMicEnabled(true)
        }
      }),
    )

    this.unsubscribers.push(
      await control.subscribe('stt_in.out', (ev) => {
        if (ev.kind !== 'text') return
        const text = String(ev.payload)
        // A fresh transcript arrives ~once per utterance. If a turn is
        // open, fill its user field; otherwise open one.
        if (s().currentTurnId === null) s().beginTurn()
        s().setUserTranscript(text)
      }),
    )

    // Server-authoritative turn lifecycle. The
    // `ConversationCoordinatorNode` emits two envelope kinds on
    // `coordinator.out`:
    //   `turn_state`    — phase transitions + turn ids
    //   `display_text`  — UI-channel text from `show(content=...)`
    // This handler dispatches both.
    this.unsubscribers.push(
      await control.subscribe('coordinator.out', (ev) => {
        if (ev.kind !== 'json') return
        const p = ev.payload as {
          kind?: string
          turn_id?: number
          phase?: string
          cancelled_turn_id?: number | null
          error?: string | null
          ts_ms?: number
          channel?: string
          text?: string
        }
        if (p.kind === 'display_text') {
          // UI text never touched TTS — it arrives here as the only
          // delivery path. Append to the current turn's liveReply so
          // it renders next to the spoken response.
          const text = p.text ?? ''
          if (text.length > 0) {
            if (s().currentTurnId === null) s().beginTurn()
            s().appendLiveReply(text)
          }
          return
        }
        if (p.kind !== 'turn_state') return
        console.log(
          '[coordinator]',
          p.phase,
          'turn',
          p.turn_id,
          p.cancelled_turn_id != null
            ? `(cancelled ${p.cancelled_turn_id})`
            : '',
          p.error ? `error=${p.error}` : '',
        )

        switch (p.phase) {
          case 'USER_SPEAKING': {
            // If a prior turn was open (either actively generating or
            // just waiting on a transcript), close it. Tag as
            // barged-in ONLY when there was in-flight assistant
            // output to cut — a fresh speech_start that lands on an
            // empty open turn just means the user kept talking.
            const { currentTurnId, turns } = s()
            if (currentTurnId !== null) {
              const active = turns.find((t) => t.id === currentTurnId)
              if (active && !active.endedAt) {
                const cancelled = p.cancelled_turn_id != null
                if (
                  cancelled &&
                  (active.generating || active.liveReply.length > 0)
                ) {
                  s().markBargedIn()
                }
                s().finalizeTurn()
              }
            }
            s().beginTurn()
            break
          }
          case 'AGENT_THINKING':
            // User turn closed; we're waiting on the LLM. No store
            // change — the stt_in.out handler will fill in the user
            // transcript on the already-open turn.
            break
          case 'AGENT_SPEAKING':
            s().setGenerating(true)
            break
          case 'IDLE': {
            const { currentTurnId, turns } = s()
            if (currentTurnId !== null) {
              const active = turns.find((t) => t.id === currentTurnId)
              if (active && !active.endedAt) {
                // The LLM has emitted <|text_end|> by now, so
                // liveReply holds the final assistant text. Copy it
                // to assistantTranscript before finalising — the
                // audio.out <|audio_end|> path is a redundant
                // safety net (fires after TTS finishes synthesising
                // the last sentence, by which point this turn is
                // already closed).
                if (active.liveReply.length > 0) {
                  s().setAssistantTranscript(active.liveReply)
                }
                if (p.error === 'llm_silence_timeout') {
                  // Timed-out turn — surface it, but don't tag
                  // barged-in (that's for user-initiated cut-offs).
                  s().setError('LLM silence timeout')
                }
                s().finalizeTurn()
              }
            }
            // Reset reply playback bookkeeping so the next turn
            // starts cleanly.
            if (this.replyReleaseTimer !== null) {
              clearTimeout(this.replyReleaseTimer)
              this.replyReleaseTimer = null
            }
            this.replyStartedAtMs = null
            this.replyEstimatedDurationMs = 0
            break
          }
          default:
            break
        }
      }),
    )

    this.unsubscribers.push(
      await control.subscribe('audio.out', (ev) => {
        // Audio envelopes: track chunk durations for the UI playback
        // indicator. We do NOT mute the mic — muting kills VAD so the
        // server can't detect the user starting to speak during
        // assistant playback (barge-in). Echo suppression is handled
        // by the browser's ``echoCancellation`` constraint on the mic
        // track, which subtracts the remote playback signal from the
        // captured mic audio so the assistant's voice doesn't loop
        // back through the room.
        if (ev.kind === 'audio') {
          const p = ev.payload as {
            size?: number
            sample_rate?: number
            channels?: number
          }
          const size = p.size ?? 0
          const sr = p.sample_rate ?? 24000
          const ch = Math.max(1, p.channels ?? 1)
          const frames = size / ch
          const chunkMs = (frames / sr) * 1000
          this.replyEstimatedDurationMs += chunkMs
          if (this.replyStartedAtMs === null) {
            this.replyStartedAtMs = performance.now()
          }
          return
        }

        if (ev.kind !== 'text') return
        const raw = String(ev.payload)
        if (raw.length === 0) return
        const isEndOfReply = raw.includes('<|audio_end|>')
        // Strip LFM2's sentinel markers before appending. Generic form
        // catches any `<|...|>` special.
        const chunk = raw.replace(/<\|[^|]*\|>/g, '')
        if (chunk.length > 0) {
          s().appendLiveReply(chunk)
        }
        if (isEndOfReply) {
          // Safety net. Coordinator.out IDLE has already closed the
          // turn and copied liveReply → assistantTranscript; this
          // branch fires later when TTS finishes its final sentence
          // and only matters if coordinator.out failed to deliver
          // (e.g. subscription dropped). In the normal case
          // currentTurnId is null here and the calls no-op.
          const { turns, currentTurnId } = s()
          if (currentTurnId !== null) {
            const t = turns.find((x) => x.id === currentTurnId)
            if (t && !t.endedAt) {
              if (t.liveReply.length > 0) {
                s().setAssistantTranscript(t.liveReply)
              }
              s().finalizeTurn()
            }
          }
          this.replyStartedAtMs = null
          this.replyEstimatedDurationMs = 0
        }
      }),
    )
  }

  // LLM-side aux ports (context / system_prompt / reset) need to reach
  // the model that actually owns the chat history. LFM2 pipelines put
  // everything on one `audio` node, so `audio.in.*` is the right topic
  // there. Qwen S2S pipelines split into `llm` + `audio`, so the
  // context / persona lives on `llm`. We publish to BOTH topics and
  // let the node the server actually has pick it up — the other
  // publish is silently dropped by the control bus when no matching
  // node exists.
  async injectKnowledge(text: string): Promise<void> {
    await Promise.all([
      this.control.publishText('audio.in.context', text),
      this.control.publishText('llm.in.context', text),
    ])
    useStore.getState().addKnowledge(text)
  }

  async setSystemPrompt(text: string): Promise<void> {
    await Promise.all([
      this.control.publishText('audio.in.system_prompt', text),
      this.control.publishText('llm.in.system_prompt', text),
    ])
  }

  async resetHistory(): Promise<void> {
    await Promise.all([
      this.control.publishText('audio.in.reset', ''),
      this.control.publishText('llm.in.reset', ''),
      // coordinator's reset wipes its turn state + text buffer,
      // publishes a turn_state envelope with error="reset" so the
      // UI can trace the cause if it subscribes to it.
      this.control.publishText('coordinator.in.reset', ''),
    ])
    useStore.getState().reset()
  }

  // Manual barge-in (user clicked a UI button without speaking).
  // Routes through `coordinator.in.barge_in` so the server-side
  // ConversationCoordinatorNode advances its turn_id, fans barge-in
  // to `llm` + `audio`, and pings its flush-audio hook to drain the
  // WebRTC outbound ring buffer — same state-machine path as the
  // VAD-driven barge.
  async bargeIn(): Promise<void> {
    await this.control.publishText('coordinator.in.barge_in', 'barge')
    useStore.getState().markBargedIn()
  }

  stop() {
    if (this.replyReleaseTimer !== null) {
      clearTimeout(this.replyReleaseTimer)
      this.replyReleaseTimer = null
    }
    this.replyStartedAtMs = null
    this.replyEstimatedDurationMs = 0
    for (const u of this.unsubscribers.splice(0)) {
      try {
        u()
      } catch {
        /* ignore */
      }
    }
    this.control.dispose()
    this.peer.close()
    this.ws.close()
    this.localStream?.getTracks().forEach((t) => t.stop())
    this.localStream = null
    useStore.getState().setLocalStream(null)
    useStore.getState().setStatus('idle')
    useStore.getState().setRemoteAudioStream(null)
  }
}
