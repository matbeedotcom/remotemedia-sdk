// Session manager — wires SignalingClient + WebRtcPeer + ControlBusClient
// together, subscribes to the right control-bus topics, and feeds events
// into the Zustand store.
//
// The heuristic for turn lifecycle is:
//   vad.is_speech_start      -> begin a turn (if none open). Fire barge_in
//                               if the assistant is still generating and
//                               autoBargeIn is on.
//   stt_in.out                -> set user transcript on the current turn
//   audio.out (kind=text)     -> append to liveReply, mark generating
//   stt_out.out               -> set assistant transcript + finalize turn
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

  // Half-duplex echo suppression. Browser AEC can't scrub the
  // room-reflected signal of the assistant's reply when it comes
  // back through external speakers, so the mic otherwise picks up
  // the model's own voice and Whisper transcribes it as a new user
  // turn. We mute the mic when the first audio chunk of a reply
  // arrives and release it once we estimate the reply has finished
  // playing out locally.
  private micOriginallyEnabled = true
  private replyStartedAtMs: number | null = null
  private replyEstimatedDurationMs = 0
  private replyReleaseTimer: ReturnType<typeof setTimeout> | null = null
  // Extra grace on top of the computed playback duration. Accounts
  // for WebRTC jitter-buffer delay and speaker→mic reverb tail.
  private readonly REPLY_GRACE_MS = 1200

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
      // AEC / noiseSuppression are DISABLED on purpose. With AEC on,
      // Chrome subtracts any mic audio that resembles currently-playing
      // audio (the assistant's reply), which means when the user
      // tries to barge in while the bot is talking, their voice is
      // classified as echo and removed before it's transmitted —
      // the server-side VAD never fires and interruptions are
      // impossible. noiseSuppression is similarly aggressive and can
      // gate out short/quiet speech.
      //
      // Trade-off: on open speakers the assistant's voice now echoes
      // back through the mic and the server will transcribe some of
      // its own output. USE HEADPHONES.
      this.localStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: false,
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
          // UNCONDITIONAL barge-in. Any speech_start from VAD halts
          // in-flight generation on every node in the pipeline. No
          // client-side gate (no `active.generating`, no
          // `autoBargeIn` toggle) — by the time VAD says the user is
          // speaking, the assistant MUST shut up and a new turn
          // starts. If nothing was generating the barge-in is a
          // harmless no-op on the server (request_barge_in just
          // raises a flag that's cleared at the start of the next
          // real synthesis).
          console.log('[session] VAD speech_start → barge-in')
          control
            .publishText('audio.in.barge_in', 'barge')
            .catch((e) => console.warn('barge_in (audio) publish failed:', e))
          control
            .publishText('llm.in.barge_in', 'barge')
            .catch((e) => console.warn('barge_in (llm) publish failed:', e))
          control
            .flushAudio()
            .catch((e) => console.warn('flushAudio failed:', e))

          const { currentTurnId, turns } = s()
          const active = currentTurnId
            ? turns.find((t) => t.id === currentTurnId)
            : null
          if (active && !active.endedAt) {
            // Only claim "barged-in" if we actually observed the
            // assistant producing output. Tagging an open turn that
            // never got a token as barged-in is misleading — the user
            // just spoke again before any reply arrived. The barge
            // publishes above are still unconditional (harmless no-op
            // server-side if nothing was generating).
            if (active.generating || active.liveReply.length > 0) {
              s().markBargedIn()
            }
            s().finalizeTurn()
          }
          if (this.replyReleaseTimer !== null) {
            clearTimeout(this.replyReleaseTimer)
            this.replyReleaseTimer = null
          }
          this.replyStartedAtMs = null
          this.replyEstimatedDurationMs = 0
          this.setMicEnabled(true)
          s().beginTurn()
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

    this.unsubscribers.push(
      await control.subscribe('audio.out', (ev) => {
        // Audio envelopes (kind=audio): optionally gate the mic so
        // we don't re-transcribe our own reply. Only do the mute
        // when the user has disabled autoBargeIn — the mute makes
        // voice barge-in impossible (no mic upload = no server VAD
        // = no audio.in.barge_in), so the two modes trade off:
        //   autoBargeIn ON  → mic stays live, voice barge-in works,
        //                     only safe with headphones (no speaker
        //                     echo to re-prime the pipeline).
        //   autoBargeIn OFF → mic is muted during replies, no voice
        //                     barge-in but safe on open speakers.
        if (ev.kind === 'audio') {
          // Track chunk durations for the UI playback indicator but
          // NEVER mute the mic. Muting kills VAD so the server can't
          // detect the user starting to speak during assistant
          // playback — which is exactly when barge-in is needed.
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
          const { turns, currentTurnId } = s()
          const t = currentTurnId
            ? turns.find((x) => x.id === currentTurnId)
            : null
          if (t) s().setAssistantTranscript(t.liveReply)
          s().finalizeTurn()
          // Reset playback tracking for the next reply. No mic-unmute
          // timer — we never muted the mic in the first place.
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
    ])
    useStore.getState().reset()
  }

  // Barge-in fires on BOTH nodes: the LLM should stop generating the
  // next token and the TTS should drop any remaining synthesis +
  // already-queued audio frames. For LFM2 only `audio.in.barge_in`
  // reaches a real node; the `llm.in` publish is a no-op there.
  async bargeIn(): Promise<void> {
    await Promise.all([
      this.control.publishText('audio.in.barge_in', 'barge'),
      this.control.publishText('llm.in.barge_in', 'barge'),
    ])
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
