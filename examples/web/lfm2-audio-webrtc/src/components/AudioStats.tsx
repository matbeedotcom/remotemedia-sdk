// Live audio diagnostics.
//
// Three meters to answer "what is the pipeline hearing?":
//
//  - MIC (local)    — WebAudio RMS of the browser's outbound mic track.
//                     Shows silence / speech level on the device side,
//                     BEFORE anything hits WebRTC / the server. If this
//                     shows activity while you're not speaking, the
//                     mic itself is picking something up.
//
//  - IN  (server)   — RMS + peak that the Silero VAD node measured on
//                     the audio it just consumed (post-resample,
//                     post-mono-downmix). Comes from the `vad.out`
//                     control-bus tap. Divergence from MIC means
//                     something got added between the browser and
//                     the pipeline.
//
//  - BOT (remote)   — WebAudio RMS of the incoming assistant audio
//                     track. Confirms playback actually happens on
//                     the expected timeline (and the mute-while-bot-
//                     is-speaking grace fires correctly).
//
// Toggle with the little "stats" pill in the StatusBar.

import { useEffect, useRef, useState } from 'react'
import clsx from 'clsx'
import { useStore } from '../store'

function dbFromRms(rms: number): number {
  if (rms <= 0) return -Infinity
  return 20 * Math.log10(rms)
}

function clampBar(db: number): number {
  // Map -80 dB..0 dB → 0..100 %
  const pct = ((db + 80) / 80) * 100
  return Math.max(0, Math.min(100, pct))
}

function Bar({ label, rms, accent }: { label: string; rms: number; accent: string }) {
  const db = dbFromRms(rms)
  const w = clampBar(db)
  return (
    <div className="flex items-center gap-2 text-[11px] font-mono">
      <span className="tag w-10 text-slate-400">{label}</span>
      <div className="flex-1 h-2 rounded bg-slate-800 overflow-hidden relative">
        <div
          className={clsx('absolute inset-y-0 left-0 transition-[width] duration-75', accent)}
          style={{ width: `${w}%` }}
        />
      </div>
      <span className="w-14 text-right text-slate-400 tabular-nums">
        {isFinite(db) ? `${db.toFixed(1)} dB` : '−∞ dB'}
      </span>
      <span className="w-16 text-right text-slate-500 tabular-nums">
        rms {rms.toFixed(4)}
      </span>
    </div>
  )
}

function useStreamRms(stream: MediaStream | null, muted: boolean): number {
  const [rms, setRms] = useState(0)
  const rafRef = useRef<number | null>(null)
  const ctxRef = useRef<AudioContext | null>(null)

  useEffect(() => {
    if (!stream || muted) {
      setRms(0)
      return
    }
    const Ctx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext
    const ctx = new Ctx()
    ctxRef.current = ctx
    const source = ctx.createMediaStreamSource(stream)
    const analyser = ctx.createAnalyser()
    analyser.fftSize = 2048
    analyser.smoothingTimeConstant = 0.4
    source.connect(analyser)
    const buf = new Float32Array(analyser.fftSize)

    const tick = () => {
      analyser.getFloatTimeDomainData(buf)
      let sum = 0
      for (let i = 0; i < buf.length; i++) {
        sum += buf[i] * buf[i]
      }
      setRms(Math.sqrt(sum / buf.length))
      rafRef.current = requestAnimationFrame(tick)
    }
    tick()
    return () => {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current)
      rafRef.current = null
      try {
        source.disconnect()
      } catch {
        /* ignore */
      }
      ctx.close().catch(() => {})
      ctxRef.current = null
      setRms(0)
    }
  }, [stream, muted])

  return rms
}

export function AudioStats() {
  const [open, setOpen] = useState(false)
  const localStream = useStore((s) => s.localStream)
  const remoteStream = useStore((s) => s.remoteAudioStream)
  const vad = useStore((s) => s.vad)

  // The mic meter wants to keep reading even while we've "muted"
  // the track server-ward — reading the MediaStream locally still
  // works because track.enabled=false only zeroes the outbound
  // packets, not the WebAudio source.
  const micRms = useStreamRms(localStream, !localStream)
  const botRms = useStreamRms(remoteStream, !remoteStream)

  if (!open) {
    return (
      <button
        type="button"
        className="pill-off text-[10px]"
        onClick={() => setOpen(true)}
        title="Show live audio diagnostics"
      >
        stats
      </button>
    )
  }

  return (
    <div className="fixed bottom-2 left-2 z-20 w-[420px] max-w-[95vw] panel p-3 flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <div className="text-xs uppercase tracking-wider text-slate-400">
          audio diagnostics
        </div>
        <button
          type="button"
          className="btn-ghost text-[10px] py-0.5 px-2"
          onClick={() => setOpen(false)}
        >
          hide
        </button>
      </div>
      <Bar label="mic" rms={micRms} accent="bg-sky-500" />
      <Bar
        label="in"
        rms={vad?.rms ?? 0}
        accent={vad?.hasSpeech ? 'bg-emerald-500' : 'bg-slate-500'}
      />
      <Bar label="bot" rms={botRms} accent="bg-amber-500" />
      <div className="text-[11px] text-slate-500 flex items-center gap-3 font-mono">
        <span>
          vad.prob{' '}
          <span
            className={clsx(
              'tabular-nums',
              vad && vad.probability >= 0.5
                ? 'text-emerald-300'
                : 'text-slate-300',
            )}
          >
            {vad ? vad.probability.toFixed(3) : '—'}
          </span>
        </span>
        <span>
          vad.peak{' '}
          <span className="tabular-nums text-slate-300">
            {vad ? vad.peak.toFixed(4) : '—'}
          </span>
        </span>
        <span>
          vad.samples{' '}
          <span className="tabular-nums text-slate-300">
            {vad?.samples ?? '—'}
          </span>
        </span>
        <span>
          @{' '}
          <span className="tabular-nums text-slate-300">
            {vad?.sampleRate ?? '—'}Hz
          </span>
        </span>
      </div>
      <div className="text-[10px] text-slate-500 leading-snug border-t border-slate-800 pt-2">
        <b>mic</b> is what your browser captures (unmuted). <b>in</b> is what
        the server's VAD just consumed (post-resample → 16 kHz mono). <b>bot</b>{' '}
        is the assistant audio arriving over WebRTC. If <b>in</b> shows activity
        while <b>mic</b> is silent, something's injecting audio between browser
        and pipeline.
      </div>
    </div>
  )
}
