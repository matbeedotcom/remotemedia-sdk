interface DemoPreviewSectionProps {
  title: string;
  bullets: string[];
}

/**
 * Demo preview section showing what the observer will detect.
 */
export function DemoPreviewSection({ title, bullets }: DemoPreviewSectionProps) {
  return (
    <section className="py-16 px-4">
      <div className="max-w-3xl mx-auto">
        <h2 className="text-2xl font-semibold text-text-primary mb-8">
          {title}
        </h2>
        <ul className="space-y-4">
          {bullets.map((bullet, index) => (
            <li
              key={index}
              className="flex items-start gap-3 text-text-secondary"
            >
              <span className="text-status-ok mt-1">✓</span>
              <span>{bullet}</span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
