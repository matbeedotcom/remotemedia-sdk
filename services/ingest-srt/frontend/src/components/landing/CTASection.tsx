import { useNavigate } from 'react-router-dom';
import type { Persona } from '@/types/persona';

interface CTASectionProps {
  persona: Persona;
  compact?: boolean;
}

/**
 * Call-to-action section with primary and secondary buttons.
 */
export function CTASection({ persona, compact = false }: CTASectionProps) {
  const navigate = useNavigate();

  const handlePrimaryCTA = () => {
    navigate(`/observe?persona=${persona.slug}`);
  };

  const handleSecondaryCTA = () => {
    navigate(`/observe?persona=${persona.slug}&demo=true`);
  };

  const handleRunLocally = () => {
    // Landing pages include the RunLocallySection; scroll there for the CLI instructions.
    document.getElementById('run-locally')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };

  if (compact) {
    return (
      <div className="flex flex-col items-center gap-4">
        <button
          onClick={handlePrimaryCTA}
          className="px-8 py-3 bg-status-info hover:bg-status-info/90 text-white rounded-lg font-medium transition-colors"
        >
          Observe a live stream →
        </button>
        <button
          onClick={handleSecondaryCTA}
          className="text-sm text-text-muted hover:text-text-secondary transition-colors"
        >
          Or try a known failure scenario
        </button>
      </div>
    );
  }

  return (
    <section className="py-12 px-4">
      <div className="max-w-xl mx-auto text-center">
        <button
          onClick={handlePrimaryCTA}
          className="px-8 py-4 bg-status-info hover:bg-status-info/90 text-white text-lg rounded-lg font-medium transition-colors"
        >
          Observe a live stream →
        </button>
        <p className="text-sm text-text-muted mt-4">
          No sign-up required · Read-only · Self-hostable
        </p>
        <button
          onClick={handleRunLocally}
          className="text-sm text-text-muted hover:text-text-secondary transition-colors mt-2"
        >
          Prefer local-only? Run the desktop CLI →
        </button>
        <button
          onClick={handleSecondaryCTA}
          className="text-sm text-text-muted hover:text-text-secondary transition-colors mt-4"
        >
          Or try a known failure scenario
        </button>
      </div>
    </section>
  );
}
