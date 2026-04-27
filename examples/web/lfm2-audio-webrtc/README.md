# LFM2-Audio WebRTC Observer

Turn-based voice assistant over WebRTC with a live observer UI and
asynchronous knowledge injection. Uses the Session Control Bus
(`control.*` JSON-RPC methods on the signaling WebSocket) to tap
transcripts and publish aux-port envelopes on the LFM2-Audio node
without touching the pipeline definition.

## What the UI shows

- **current turn** — live user transcript (from `stt_in.out`), live
  assistant text tokens streamed during generation (from `audio.out`),
  and the final whisper-verified assistant transcript
  (from `stt_out.out`).
- **history** — per-turn user + assistant transcripts, turn duration,
  and whether the turn was barged-in.
- **knowledge injection** — a textarea that publishes to
  `audio.in.context`. Persists across turns until you hit **reset**.
- **persona** — publishes to `audio.in.system_prompt`.
- **barge** — force-interrupts the current generation by publishing
  `audio.in.barge_in`. Also fires automatically on VAD speech-start
  while the assistant is generating (toggle with `auto barge-in`).

Remote audio from LFM2 plays through a small `<audio>` controls panel
bottom-right — WebRTC carries the audio bytes; the control bus only
carries text events.

## Run

### 1. Start the server

The demo server is an `example` binary in the webrtc crate:

```bash
# Torch backend (Linux / CUDA):
cargo run --example lfm2_audio_webrtc_server \
    -p remotemedia-webrtc --features ws-signaling -- --port 8081

# Apple Silicon MLX backend:
LFM2_AUDIO_BACKEND=mlx \
  PYTHON_ENV_MODE=managed PYTHON_VERSION=3.12 \
  REMOTEMEDIA_PYTHON_SRC="$PWD/clients/python" \
  cargo run --example lfm2_audio_webrtc_server \
      -p remotemedia-webrtc --features ws-signaling -- --port 8081
```

When you see `READY ws://127.0.0.1:8081/ws` the server is accepting
WebRTC peers.

### 2. Start the SPA

```bash
cd examples/web/lfm2-audio-webrtc
npm install
npm run dev
```

Open the URL Vite prints (default `http://localhost:5173`). The WS URL
defaults to `ws://127.0.0.1:8081/ws`; override with `?ws=...` in the
query string or change it in the header before clicking **start mic**.

### 3. Talk

Click **start mic**, allow microphone access, and speak. The VAD opens
a turn on speech-start and closes it on silence; the full utterance is
then fed to LFM2-Audio. Audio replies stream back in real time.

## Pipeline

See [`crates/transports/webrtc/examples/lfm2_audio_webrtc_server.rs`](../../../crates/transports/webrtc/examples/lfm2_audio_webrtc_server.rs):

```text
mic (48k Opus)
  │
  ▼
resample_in (48k → 16k)
  │
  ▼
vad  ────────────────────►  (vad.out: JSON per chunk)
  │
  ▼
accumulator  (buffers until silence, releases one utterance)
  │
  ├─► stt_in   (stt_in.out: user transcript)
  │
  ▼
audio  (LFM2AudioNode / LFM2AudioMlxNode)
  │
  ├─► WebRTC audio track back to browser
  │
  ▼
stt_out  (stt_out.out: assistant transcript)
```

## Control-bus wire protocol

The SPA uses the same WebSocket as signaling. Three extra JSON-RPC
methods:

- `control.subscribe { topic }` — returns `{ subscribed: true }` and
  starts emitting `control.event` notifications for that topic.
- `control.unsubscribe { topic }` — stops that topic's forwarder.
- `control.publish { topic, payload }` — injects data into a node's
  input (main or aux port).
- `control.set_node_state { node_id, state: "enabled"|"bypass"|"disabled" }`
  — flip runtime state.

Topics are `node.direction[.port]` strings. Examples:

- subscribe: `audio.out`, `vad.out`, `stt_in.out`, `stt_out.out`
- publish: `audio.in.context`, `audio.in.barge_in`, `audio.in.reset`,
  `audio.in.system_prompt`

Events come back as:

```json
{
  "jsonrpc": "2.0",
  "method": "control.event",
  "params": { "topic": "vad.out", "kind": "json", "payload": { ... }, "ts": 1234567890 }
}
```

`kind` is one of `text | json | audio | binary | other`. `audio`
events are liveness-only (`size`, `sample_rate`, `channels`) unless
the client subscribed with `include_audio: true`.
