import type { Persona } from '@/types/persona';

/**
 * All available personas for landing pages and event copy transformations.
 */
export const PERSONAS: Persona[] = [
  {
    id: 'voice-agents',
    slug: 'voice-agents',
    name: 'AI Voice Agents',
    defaultPipeline: 'ai_voice_agent_health_v1',
    hero: {
      headline: 'Know when your voice agent hesitates, interrupts, or fails to respond — in real time',
      subheadline: 'Observe live audio and video streams as a read-only side-car — without modifying your media or AI stack.',
    },
    problem: {
      title: 'Why this breaks in production',
      bullets: [
        'Agents respond too slowly and users talk over them',
        'Silent failures where the agent never speaks',
        'Broken audio output that looks "healthy" to logs',
        'No clear timeline of what actually happened',
      ],
    },
    demoShows: {
      title: 'What this demo shows',
      bullets: [
        'Agent response latency after user finishes speaking',
        'Agent interruptions and talk-over',
        'Silent agent failures (no response)',
        'Audio output quality issues',
        'Session health over time',
      ],
    },
    eventCopyMap: {
      'audio.silence': 'No agent response',
      'audio.clipping': 'Agent audio distorted',
      'audio.low_volume': 'Agent response quiet',
      'audio.channel_imbalance': 'Audio channel imbalance',
      'timing.jitter_spike': 'Agent response slow',
      'timing.clock_drift': 'Timing drift',
      'timing.lead_jump': 'Response timing jump',
      'speech.transition': 'Speech transition',
      'stream_started': 'Session started',
      'stream_ended': 'Session ended',
    },
  },
  {
    id: 'contact-center',
    slug: 'contact-center',
    name: 'Contact Center QA',
    defaultPipeline: 'contact_center_qa_v1',
    hero: {
      headline: 'Catch dead air, talk-over, and broken calls in real time',
      subheadline: 'Observe live audio and video streams as a read-only side-car — without modifying your media or AI stack.',
    },
    problem: {
      title: 'Why this breaks in production',
      bullets: [
        'Dead air kills customer satisfaction',
        'Agents talking over customers miss key information',
        'Call quality issues go undetected until complaints',
        'No visibility into real-time session health',
      ],
    },
    demoShows: {
      title: 'What this demo shows',
      bullets: [
        'Speech presence on both channels',
        'Talk ratios and overlap detection',
        'Dead air and silence patterns',
        'Session health scoring',
        'Real-time alert correlation',
      ],
    },
    eventCopyMap: {
      'audio.silence': 'Dead air detected',
      'audio.clipping': 'Agent clipping',
      'audio.low_volume': 'Low call volume',
      'audio.channel_imbalance': 'Channel imbalance',
      'timing.jitter_spike': 'Call quality degraded',
      'timing.clock_drift': 'Call timing drift',
      'timing.lead_jump': 'Call timing jump',
      'speech.transition': 'Speaker change',
      'stream_started': 'Call started',
      'stream_ended': 'Call ended',
    },
  },
  {
    id: 'telehealth',
    slug: 'telehealth',
    name: 'Telehealth',
    defaultPipeline: 'demo_audio_quality_v1',
    hero: {
      headline: 'Know when patients can\'t hear you — before the visit fails',
      subheadline: 'Observe live audio and video streams as a read-only side-car — without modifying your media or AI stack.',
    },
    problem: {
      title: 'Why this breaks in production',
      bullets: [
        'Patients disconnect when audio fails',
        'Low volume makes providers inaudible',
        'Silence looks like connection issues',
        'No way to know if the patient heard you',
      ],
    },
    demoShows: {
      title: 'What this demo shows',
      bullets: [
        'Patient and provider audio presence',
        'Low volume detection on either side',
        'Silence and dropout detection',
        'Audio quality scoring',
        'Session health indicators',
      ],
    },
    eventCopyMap: {
      'audio.silence': 'Patient audio silent',
      'audio.clipping': 'Provider audio clipping',
      'audio.low_volume': 'Patient audio quiet',
      'audio.channel_imbalance': 'Audio imbalance',
      'timing.jitter_spike': 'Connection unstable',
      'timing.clock_drift': 'Connection drift',
      'timing.lead_jump': 'Connection jump',
      'speech.transition': 'Speaker change',
      'stream_started': 'Visit started',
      'stream_ended': 'Visit ended',
    },
  },
  {
    id: 'broadcast',
    slug: 'broadcast',
    name: 'Broadcast / Streaming',
    defaultPipeline: 'demo_av_quality_v1',
    hero: {
      headline: 'Detect silence, clipping, and frozen video before viewers complain',
      subheadline: 'Observe live audio and video streams as a read-only side-car — without modifying your media or AI stack.',
    },
    problem: {
      title: 'Why this breaks in production',
      bullets: [
        'Audio dropouts during live broadcasts',
        'Clipping that distorts talent audio',
        'Black frames and frozen video',
        'No real-time quality monitoring',
      ],
    },
    demoShows: {
      title: 'What this demo shows',
      bullets: [
        'Audio dropout and silence detection',
        'Clipping and distortion alerts',
        'Video freeze and black frame detection',
        'A/V sync monitoring',
        'Stream health timeline',
      ],
    },
    eventCopyMap: {
      'audio.silence': 'Audio dropout',
      'audio.clipping': 'Audio clipping',
      'audio.low_volume': 'Low audio level',
      'audio.channel_imbalance': 'Channel imbalance',
      'timing.jitter_spike': 'Timing jitter spike',
      'timing.clock_drift': 'Clock drift',
      'timing.lead_jump': 'Timing jump',
      'speech.transition': 'Audio transition',
      'stream_started': 'Stream started',
      'stream_ended': 'Stream ended',
    },
  },
  {
    id: 'general',
    slug: 'general',
    name: 'General / Exploratory',
    defaultPipeline: 'full_stream_health_v1',
    hero: {
      headline: 'Observe the health of live media in real time',
      subheadline: 'Observe live audio and video streams as a read-only side-car — without modifying your media or AI stack.',
    },
    problem: {
      title: 'Why this breaks in production',
      bullets: [
        'Live media fails silently',
        'Log files miss audio/video issues',
        'No timeline of what actually happened',
        'Debugging requires reproduction',
      ],
    },
    demoShows: {
      title: 'What this demo shows',
      bullets: [
        'Complete audio and video analysis',
        'Speech and conversation patterns',
        'Timing and synchronization',
        'Session health scoring',
        'Event correlation and incidents',
      ],
    },
    eventCopyMap: {
      // General persona uses default event labels (no transformations)
    },
  },
];

/**
 * Get a persona by its URL slug.
 */
export function getPersonaBySlug(slug: string): Persona | undefined {
  return PERSONAS.find((p) => p.slug === slug);
}

/**
 * Get the default (general) persona.
 */
export function getDefaultPersona(): Persona {
  return PERSONAS.find((p) => p.id === 'general')!;
}
