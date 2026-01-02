import type { Persona } from '@/types/persona';
import { HeroSection } from './HeroSection';
import { ProblemSection } from './ProblemSection';
import { SolutionSection } from './SolutionSection';
import { DemoPreviewSection } from './DemoPreviewSection';
import { CTASection } from './CTASection';
import { HowItWorksSection } from './HowItWorksSection';
import { RunLocallySection } from './RunLocallySection';
import { TrustSection } from './TrustSection';

interface LandingPageProps {
  persona: Persona;
}

/**
 * Templated landing page that renders persona-specific content.
 */
export function LandingPage({ persona }: LandingPageProps) {
  return (
    <div className="min-h-screen bg-surface-primary">
      {/* Hero */}
      <HeroSection
        headline={persona.hero.headline}
        subheadline={persona.hero.subheadline}
      />

      {/* Primary CTA */}
      <CTASection persona={persona} />

      {/* Problem */}
      <ProblemSection
        title={persona.problem.title}
        bullets={persona.problem.bullets}
      />

      {/* Solution */}
      <SolutionSection />

      {/* What Demo Shows */}
      <DemoPreviewSection
        title={persona.demoShows.title}
        bullets={persona.demoShows.bullets}
      />

      {/* How It Works */}
      <HowItWorksSection personaName={persona.name} />

      {/* Run locally (CLI) */}
      <RunLocallySection />

      {/* Trust */}
      <TrustSection />

      {/* Bottom CTA */}
      <section className="py-16 px-4 text-center">
        <CTASection persona={persona} compact />
      </section>
    </div>
  );
}
