interface HowItWorksSectionProps {
  personaName: string;
}

/**
 * How it works section with 3-step guide.
 */
export function HowItWorksSection({ personaName }: HowItWorksSectionProps) {
  const steps = [
    {
      number: 1,
      title: 'Choose what you want to observe',
      description: `We preselect a pipeline for ${personaName} — you can change it anytime.`,
    },
    {
      number: 2,
      title: 'Run a single FFmpeg command',
      description:
        'This sends a copy of your stream to the service you’re connected to. Prefer local-only? Use the desktop CLI instead.',
    },
    {
      number: 3,
      title: 'Watch live events appear',
      description: 'Timeline, evidence, and webhooks fire in real time.',
    },
  ];

  return (
    <section className="py-16 px-4 bg-surface-secondary">
      <div className="max-w-3xl mx-auto">
        <h2 className="text-2xl font-semibold text-text-primary mb-10 text-center">
          How the demo works
        </h2>
        <div className="space-y-8">
          {steps.map((step) => (
            <div key={step.number} className="flex gap-6">
              <div className="flex-shrink-0 w-10 h-10 rounded-full bg-status-info/20 text-status-info flex items-center justify-center font-bold">
                {step.number}
              </div>
              <div>
                <h3 className="font-medium text-text-primary mb-1">
                  {step.title}
                </h3>
                <p className="text-text-secondary text-sm">
                  {step.description}
                </p>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
