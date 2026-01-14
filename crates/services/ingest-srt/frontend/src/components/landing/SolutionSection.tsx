/**
 * Solution section explaining the side-car observation model.
 * This content is shared across all personas.
 */
export function SolutionSection() {
  return (
    <section className="py-16 px-4 bg-surface-secondary">
      <div className="max-w-3xl mx-auto">
        <h2 className="text-2xl font-semibold text-text-primary mb-6">
          Side-car observation for live media
        </h2>
        <p className="text-text-secondary mb-8">
          RemoteMedia attaches <strong className="text-text-primary">beside</strong> your
          existing media pipeline and observes a copy of the stream in real time.
        </p>
        <ul className="space-y-3">
          <li className="flex items-center gap-3 text-text-secondary">
            <span className="text-status-ok">✓</span>
            <span>No client changes</span>
          </li>
          <li className="flex items-center gap-3 text-text-secondary">
            <span className="text-status-ok">✓</span>
            <span>No model instrumentation</span>
          </li>
          <li className="flex items-center gap-3 text-text-secondary">
            <span className="text-status-ok">✓</span>
            <span>No impact on latency</span>
          </li>
          <li className="flex items-center gap-3 text-text-secondary">
            <span className="text-status-ok">✓</span>
            <span>No control over your media path</span>
          </li>
        </ul>
        <p className="text-text-muted mt-8 text-sm">
          Just facts, events, and evidence.
        </p>
      </div>
    </section>
  );
}
