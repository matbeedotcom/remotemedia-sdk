interface HeroSectionProps {
  headline: string;
  subheadline: string;
}

/**
 * Hero section with persona-specific headline and shared subheadline.
 */
export function HeroSection({ headline, subheadline }: HeroSectionProps) {
  return (
    <section className="py-20 px-4">
      <div className="max-w-4xl mx-auto text-center">
        <h1 className="text-4xl md:text-5xl font-bold text-text-primary mb-6 leading-tight">
          {headline}
        </h1>
        <p className="text-xl text-text-secondary max-w-2xl mx-auto">
          {subheadline}
        </p>
      </div>
    </section>
  );
}
