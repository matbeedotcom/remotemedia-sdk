import { useEffect, useRef } from 'react'
import { useStore } from '../store'

/// Hidden <audio> element that plays the assistant's reply stream
/// coming back over the WebRTC peer's remote audio track.
export function RemoteAudio() {
  const stream = useStore((s) => s.remoteAudioStream)
  const ref = useRef<HTMLAudioElement>(null)

  useEffect(() => {
    const el = ref.current
    if (!el) return
    el.srcObject = stream ?? null
    if (stream) {
      el.play().catch((e) => {
        // Autoplay can be blocked until the user interacts with the
        // page — the start button already counts as a gesture but
        // some browsers need a second nudge.
        console.warn('[audio] autoplay blocked:', e)
      })
    }
  }, [stream])

  return (
    <audio
      ref={ref}
      autoPlay
      playsInline
      // Keep it off-screen but not display:none — Safari pauses
      // display:none audio elements.
      className="fixed bottom-2 right-2 w-40 opacity-70"
      controls
    />
  )
}
