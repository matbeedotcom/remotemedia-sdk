/**
 * "Run locally" CTA section for users who don't want to stream media to a service.
 *
 * This points at the desktop demo CLI (`remotemedia-demo`) which emits the same
 * event stream (JSONL) that the observer UI visualizes.
 */
export function RunLocallySection() {
  const buildAndRun = `cd examples/cli/stream-health-demo
cargo build --release
./target/release/remotemedia-demo --ingest file:///path/to/input.mp4 --json -q`;

  const ffmpegPipe = `ffmpeg -hide_banner -loglevel warning -i input.mp4 -f f32le -ar 16000 -ac 1 - \\
  | ./target/release/remotemedia-demo -i - --stream --json -q`;

  return (
    <section id="run-locally" className="py-16 px-4">
      <div className="max-w-3xl mx-auto">
        <h2 className="text-2xl font-semibold text-text-primary mb-3">
          Prefer to keep media local?
        </h2>
        <p className="text-text-secondary mb-8">
          You can run the desktop CLI and get the <strong className="text-text-primary">same events</strong> as this UI
          (JSONL) without sending your audio/video to any remote service.
        </p>

        <div className="grid gap-6">
          <div className="rounded-xl border border-surface-elevated bg-surface-secondary p-5">
            <p className="text-sm font-medium text-text-primary mb-3">Build + run on a local file</p>
            <pre className="p-4 bg-surface-primary rounded-lg text-xs font-mono overflow-x-auto whitespace-pre-wrap break-all text-text-secondary">
              {buildAndRun}
            </pre>
            <p className="text-xs text-text-muted mt-3">
              Tip: replace <span className="font-mono">input.mp4</span> with any local source you already have.
            </p>
          </div>

          <div className="rounded-xl border border-surface-elevated bg-surface-secondary p-5">
            <p className="text-sm font-medium text-text-primary mb-3">Or pipe from FFmpeg (no files on disk)</p>
            <pre className="p-4 bg-surface-primary rounded-lg text-xs font-mono overflow-x-auto whitespace-pre-wrap break-all text-text-secondary">
              {ffmpegPipe}
            </pre>
            <p className="text-xs text-text-muted mt-3">
              This is great for “side-car” monitoring: your primary stream can continue unchanged.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

