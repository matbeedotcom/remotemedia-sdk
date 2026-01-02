import { Navigate } from 'react-router-dom';
import { getPersonaBySlug, getDefaultPersona } from '@/config/personas';
import { LandingPage } from '@/components/landing/LandingPage';

interface LandingRouteProps {
  slug: string;
}

/**
 * Route wrapper that resolves persona by slug and renders LandingPage.
 * Redirects to root if persona not found.
 */
export function LandingRoute({ slug }: LandingRouteProps) {
  const persona = getPersonaBySlug(slug) ?? getDefaultPersona();

  if (!persona) {
    return <Navigate to="/" replace />;
  }

  return <LandingPage persona={persona} />;
}
