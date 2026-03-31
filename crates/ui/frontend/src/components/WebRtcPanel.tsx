export function WebRtcPanel({ transport }: { transport: any }) {
  return (
    <div class="panel">
      <h2>WebRTC Real-Time</h2>
      <p>Connect to the WebRTC signaling server for real-time audio streaming.</p>
      <p class="transport-url">Signaling: {transport?.transport_url || 'unknown'}</p>
      <button class="btn" disabled>Connect (coming soon)</button>
    </div>
  );
}
