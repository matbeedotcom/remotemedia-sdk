/**
 * Trust and safety section with production-ready messaging.
 */
export function TrustSection() {
  const trustPoints = [
    'Read-only observation',
    'No media modification',
    'No persistent storage by default',
    'All logic visible in pipeline manifests',
  ];

  return (
    <section className="py-16 px-4">
      <div className="max-w-3xl mx-auto text-center">
        <h2 className="text-xl font-semibold text-text-primary mb-6">
          Built for production reality
        </h2>
        <ul className="flex flex-wrap justify-center gap-x-8 gap-y-3">
          {trustPoints.map((point, index) => (
            <li
              key={index}
              className="flex items-center gap-2 text-text-secondary text-sm"
            >
              <span className="text-status-ok">✓</span>
              <span>{point}</span>
            </li>
          ))}
        </ul>
        <p className="text-text-muted text-xs mt-8">
          This demo uses the same runtime and event model as production deployments.
        </p>
      </div>
    </section>
  );
}
