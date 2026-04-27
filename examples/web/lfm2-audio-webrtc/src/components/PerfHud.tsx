// Per-node performance HUD.
//
// Renders the sticky `perfNodes` map (last meaningful stats per node
// + age) from the server's `__perf__.out` tap as a one-row-per-node
// table. The "TTFT" column shows first-output latency (input arrival
// → first emission) — the meaningful number for streaming nodes
// (LLM token stream, TTS chunk stream).
//
// Why sticky rather than "latest snapshot"
// ----------------------------------------
// The server's perf aggregator resets HDR histograms every window
// (default 1 s) so a node that only fires once every few seconds
// (kokoro_tts, the LLM, etc.) would *vanish* on the next idle tick
// if the HUD only displayed the current snapshot. The store's
// `ingestPerfSnapshot` merges activity into a sticky map per node,
// and we render that, dimming rows whose stats are >5 s old.
//
// Server emits these only when started with `REMOTEMEDIA_PERF_TAP=1`.
// When disabled, `perfSnapshot` stays null and the HUD shows the
// "idle" hint rather than empty space, so it's obvious the feature
// is off rather than broken.

import clsx from 'clsx'
import { useEffect, useState } from 'react'
import { useStore } from '../store'

function fmtMs(us: number): string {
  if (us === 0) return '—'
  if (us < 1_000) return `${us}µs`
  const ms = us / 1000
  if (ms < 10) return `${ms.toFixed(1)}ms`
  if (ms < 1000) return `${Math.round(ms)}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function fmtAge(ms: number): string {
  if (ms < 0) return 'now'
  if (ms < 1000) return `${ms}ms`
  if (ms < 60_000) return `${Math.round(ms / 1000)}s`
  if (ms < 3_600_000) return `${Math.round(ms / 60_000)}m`
  return `${Math.round(ms / 3_600_000)}h`
}

const STALE_AFTER_MS = 5_000

export function PerfHud() {
  const snap = useStore((s) => s.perfSnapshot)
  const rows = useStore((s) => s.perfNodes)
  const clear = useStore((s) => s.clearPerfNodes)

  // Tick a local clock every 500 ms so the "ago" column updates
  // smoothly even between server snapshots (which may be 1+ s
  // apart). Cheap — only re-renders the HUD.
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 500)
    return () => clearInterval(t)
  }, [])

  // Transient "copied!" indicator. Cleared by a setTimeout in the
  // copy handler — no need for a separate effect.
  const [copyState, setCopyState] = useState<'idle' | 'ok' | 'err'>('idle')
  const handleCopy = async () => {
    // Snapshot what's on screen — sticky rows plus the latest
    // server snapshot envelope, so the copied JSON is self-
    // describing for sharing in bug reports.
    const payload = {
      latest_snapshot: snap,
      sticky_rows: rows,
      copied_at_ms: Date.now(),
    }
    const json = JSON.stringify(payload, null, 2)
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(json)
      } else {
        // Fallback for non-secure contexts (e.g. http:// pages).
        const ta = document.createElement('textarea')
        ta.value = json
        ta.style.position = 'fixed'
        ta.style.opacity = '0'
        document.body.appendChild(ta)
        ta.select()
        document.execCommand('copy')
        document.body.removeChild(ta)
      }
      setCopyState('ok')
    } catch (e) {
      console.warn('[perf-hud] copy failed:', e)
      setCopyState('err')
    }
    setTimeout(() => setCopyState('idle'), 1500)
  }

  const entries = Object.entries(rows)
  if (!snap && entries.length === 0) {
    return (
      <div className="rounded border border-zinc-800 bg-zinc-950/70 p-3 text-xs text-zinc-500">
        <div className="font-mono uppercase tracking-wide text-zinc-400">
          perf · idle
        </div>
        <div className="mt-1">
          No snapshots yet. Set{' '}
          <code className="rounded bg-zinc-800 px-1">REMOTEMEDIA_PERF_TAP=1</code>{' '}
          on the server to enable.
        </div>
      </div>
    )
  }

  // Sort: freshest first, then alphabetical within the same age bucket.
  const sorted = entries.sort(([a, ra], [b, rb]) => {
    if (rb.lastActivityMs !== ra.lastActivityMs) {
      return rb.lastActivityMs - ra.lastActivityMs
    }
    return a.localeCompare(b)
  })

  return (
    <div className="rounded border border-zinc-800 bg-zinc-950/70 p-3 text-xs">
      <div className="mb-2 flex items-baseline justify-between font-mono uppercase tracking-wide text-zinc-400">
        <span>
          perf · {snap ? `${snap.window_ms}ms window` : 'sticky'}
        </span>
        <span className="flex items-center gap-2 text-zinc-600">
          {snap && <span>{new Date(snap.ts_ms).toLocaleTimeString()}</span>}
          <button
            onClick={handleCopy}
            disabled={!snap && Object.keys(rows).length === 0}
            className={clsx(
              'rounded border px-2 py-0.5 text-[10px] uppercase tracking-wide transition-colors',
              copyState === 'ok'
                ? 'border-emerald-700 text-emerald-400'
                : copyState === 'err'
                  ? 'border-rose-700 text-rose-400'
                  : 'border-zinc-800 text-zinc-500 hover:border-zinc-700 hover:text-zinc-300',
              'disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:border-zinc-800',
            )}
            title="Copy the perf table as JSON"
          >
            {copyState === 'ok'
              ? 'copied'
              : copyState === 'err'
                ? 'failed'
                : 'copy'}
          </button>
          <button
            onClick={clear}
            className="rounded border border-zinc-800 px-2 py-0.5 text-[10px] uppercase tracking-wide text-zinc-500 hover:border-zinc-700 hover:text-zinc-300"
            title="Forget all node history"
          >
            clear
          </button>
        </span>
      </div>
      {sorted.length === 0 ? (
        <div className="text-zinc-500">No node activity recorded.</div>
      ) : (
        <table className="w-full font-mono text-[11px] tabular-nums">
          <thead className="text-zinc-500">
            <tr>
              <th className="text-left">node</th>
              <th className="text-right">in</th>
              <th className="text-right">out</th>
              <th className="text-right">ttft p50</th>
              <th className="text-right">p50</th>
              <th className="text-right">p95</th>
              <th className="text-right">p99</th>
              <th className="text-right">last</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map(([id, row]) => {
              const ageMs = Math.max(0, now - row.lastActivityMs)
              const stale = ageMs > STALE_AFTER_MS
              return (
                <tr
                  key={id}
                  className={clsx(
                    'border-t border-zinc-900 transition-opacity',
                    stale ? 'text-zinc-600' : 'text-zinc-300',
                  )}
                >
                  <td
                    className={clsx(
                      'py-0.5 pr-2',
                      stale ? 'text-zinc-500' : 'text-zinc-200',
                    )}
                  >
                    {id}
                  </td>
                  <td className="text-right">{row.stats.inputs}</td>
                  <td className="text-right">{row.stats.outputs}</td>
                  <td className="text-right">
                    {fmtMs(row.stats.first_output_latency_us.p50_us)}
                  </td>
                  <td className="text-right">
                    {fmtMs(row.stats.latency_us.p50_us)}
                  </td>
                  <td className="text-right">
                    {fmtMs(row.stats.latency_us.p95_us)}
                  </td>
                  <td className="text-right">
                    {fmtMs(row.stats.latency_us.p99_us)}
                  </td>
                  <td
                    className={clsx(
                      'text-right',
                      stale ? 'text-zinc-600' : 'text-emerald-500',
                    )}
                    title={`Last activity ${new Date(row.lastActivityMs).toLocaleTimeString()}`}
                  >
                    {fmtAge(ageMs)}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      )}
    </div>
  )
}
