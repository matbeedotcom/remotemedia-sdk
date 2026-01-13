import { useSessionStore } from '@/store/session';
import { getEventCategory, type EventCategory } from '@/types/events';
import clsx from 'clsx';

const categoryColors: Record<EventCategory, string> = {
  speech: 'border-accent-speech',
  conversation: 'border-accent-conversation',
  health: 'border-status-ok',
  timing: 'border-accent-timing',
  alert: 'border-status-warning',
  incident: 'border-status-error',
  system: 'border-text-muted',
  event: 'border-text-secondary',
};

export function EvidencePane() {
  const selectedEventId = useSessionStore((s) => s.selectedEventId);
  const events = useSessionStore((s) => s.events);

  const selectedEvent = selectedEventId ? events.find((e) => e.id === selectedEventId) : null;

  if (!selectedEvent) {
    return (
      <div className="h-full flex flex-col">
        {/* Header */}
        <div className="p-4 border-b border-surface-elevated">
          <h2 className="text-sm font-medium text-text-muted">Event details</h2>
        </div>
        {/* Empty state hint */}
        <div className="flex-1 flex items-center justify-center p-8">
          <p className="text-sm text-text-muted text-center">
            Select an event from the timeline to inspect it.
          </p>
        </div>
      </div>
    );
  }

  const category = getEventCategory(selectedEvent.event_type);

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* Header */}
      <div className={clsx('p-4 border-b border-surface-elevated', categoryColors[category])}>
        <h2 className="text-lg font-semibold text-text-primary">{getEventTitle(selectedEvent)}</h2>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {/* Description - what happened */}
        <section>
          <p className="text-sm text-text-primary leading-relaxed">
            {getEventMeta(selectedEvent).description}
          </p>
        </section>

        {/* What it means */}
        <section>
          <h3 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-2">
            What this means
          </h3>
          <p className="text-sm text-text-secondary leading-relaxed">
            {getEventMeta(selectedEvent).meaning}
          </p>
        </section>

        {/* Details section - specific metrics */}
        {renderEventDetails(selectedEvent).length > 0 && (
          <section>
            <h3 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3">
              Metrics
            </h3>
            <div className="space-y-2">
              {renderEventDetails(selectedEvent)}
            </div>
          </section>
        )}

        {/* Likely causes (for certain events) */}
        {renderEventContext(selectedEvent)}

        {/* Raw data (collapsed by default) */}
        <section>
          <details className="group">
            <summary className="text-xs font-medium text-text-muted cursor-pointer hover:text-text-secondary">
              View raw event data
            </summary>
            <pre className="mt-2 p-3 bg-surface-primary rounded-lg text-xs font-mono text-text-secondary overflow-x-auto">
              {JSON.stringify(selectedEvent, null, 2)}
            </pre>
          </details>
        </section>
      </div>
    </div>
  );
}

/** Event metadata: title, description, and what it means */
interface EventMeta {
  title: string;
  description: string;
  meaning: string;
}

/** Get metadata for event type */
function getEventMeta(event: Record<string, unknown>): EventMeta {
  const eventType = event.event_type as string;

  switch (eventType) {
    // System events
    case 'stream_started':
      return {
        title: 'Stream started',
        description: 'The media stream connection was established and data is now flowing.',
        meaning: 'Your stream is live and being analyzed. Audio and video data are being received.',
      };
    case 'stream_ended':
      return {
        title: 'Stream ended',
        description: `The stream was disconnected. Reason: ${(event.reason as string) || 'unknown'}.`,
        meaning: 'The streaming session has concluded. Review the timeline for any issues that occurred during the stream.',
      };

    // Drift/timing alerts from HealthEmitterNode
    case 'drift':
      return {
        title: 'A/V drift detected',
        description: `Audio and video timestamps are drifting apart. Current lead: ${event.lead_ms}ms (threshold: ${event.threshold_ms}ms).`,
        meaning: 'Audio and video are becoming increasingly out of sync over time. This typically indicates clock rate differences between audio and video sources, or processing delays accumulating.',
      };
    case 'av_skew':
      return {
        title: 'A/V skew detected',
        description: `Audio/video synchronization offset is ${event.skew_ms}ms (threshold: ${event.threshold_ms}ms).`,
        meaning: 'Audio and video are not aligned - viewers may notice lips not matching speech. This can be caused by encoding delays, network jitter, or source timing issues.',
      };
    case 'cadence':
      return {
        title: 'Frame cadence unstable',
        description: `Frame timing variation (CV: ${typeof event.cv === 'number' ? (event.cv * 100).toFixed(0) : '?'}%) exceeds threshold.`,
        meaning: 'Video frames are arriving at irregular intervals instead of a steady rate. This causes stuttering or jerky playback. Common causes: CPU overload, network congestion, or encoder issues.',
      };
    case 'freeze':
      return {
        title: 'Stream freeze detected',
        description: `No new frames received for ${typeof event.duration_ms === 'number' ? (event.duration_ms / 1000).toFixed(1) : '?'} seconds.`,
        meaning: 'The video stream stopped updating - viewers see a frozen image. This indicates a complete interruption in video data, often due to network dropout or encoder crash.',
      };
    case 'health':
      return {
        title: 'Health score update',
        description: `Overall stream health: ${typeof event.score === 'number' ? (event.score * 100).toFixed(0) : '?'}%.`,
        meaning: 'This is a composite score combining all quality metrics. A declining score indicates accumulating issues that may affect viewer experience.',
      };

    // Audio quality events (from HealthEmitterNode)
    case 'silence':
      return {
        title: 'Silence detected',
        description: `Audio level dropped below threshold for ${typeof event.duration_ms === 'number' ? (event.duration_ms / 1000).toFixed(1) : '?'} seconds.`,
        meaning: 'No audio signal is being transmitted. Check if the microphone is muted, disconnected, or if there\'s truly no sound at the source.',
      };
    case 'clipping':
      return {
        title: 'Audio clipping',
        description: `Audio signal is saturating (${typeof event.saturation_ratio === 'number' ? (event.saturation_ratio * 100).toFixed(1) : '?'}% of samples at max).`,
        meaning: 'The audio is too loud and distorting. Reduce microphone gain or input volume to prevent harsh, distorted sound for listeners.',
      };
    case 'low_volume':
      return {
        title: 'Low volume detected',
        description: `Audio level is ${typeof event.rms_db === 'number' ? event.rms_db.toFixed(0) : '?'} dB (very quiet).`,
        meaning: 'Audio is too quiet for comfortable listening. Increase microphone gain or move closer to the microphone.',
      };
    case 'channel_imbalance':
      return {
        title: 'Channel imbalance',
        description: `Stereo channels differ by ${typeof event.imbalance_db === 'number' ? event.imbalance_db.toFixed(0) : '?'} dB. ${event.dead_channel !== 'none' ? `Dead channel: ${event.dead_channel}` : ''}`,
        meaning: 'Left and right audio channels have very different levels. This often indicates a mono source incorrectly connected to a stereo input, or a faulty cable.',
      };
    case 'dropouts':
      return {
        title: 'Audio dropouts',
        description: `${event.dropout_count || '?'} brief audio interruptions detected in monitoring window.`,
        meaning: 'Short gaps are appearing in the audio stream. This creates choppy, broken audio. Usually caused by network packet loss or buffer underruns.',
      };

    // Namespaced audio events
    case 'audio.silence':
      return {
        title: 'Silence detected',
        description: 'Extended period of no audio detected.',
        meaning: 'No audio signal is being transmitted. This may indicate a muted microphone, technical issue, or intentional pause.',
      };
    case 'audio.clipping':
      return {
        title: 'Audio clipping detected',
        description: 'Audio signal exceeded maximum level, causing distortion.',
        meaning: 'The audio is too loud and distorting. Listeners hear harsh, clipped sounds. Reduce microphone gain or input volume.',
      };
    case 'audio.channel_imbalance':
      return {
        title: 'Channel imbalance',
        description: 'Significant level difference between left and right channels.',
        meaning: 'Stereo audio is unbalanced. This may indicate a mono source connected to stereo input, or a wiring issue.',
      };
    case 'audio.low_volume':
      return {
        title: 'Low volume detected',
        description: 'Audio level is consistently below optimal range.',
        meaning: 'Audio is too quiet for comfortable listening. Increase microphone gain or speak closer to the microphone.',
      };

    // Speech events
    case 'speech.transition':
    case 'speech.state':
      return {
        title: 'Speech state change',
        description: `Speech transitioned to: ${event.state || 'unknown'}.`,
        meaning: getSpeechStateMeaning(event.state as string),
      };

    // Conversation events
    case 'conversation.flow':
      return {
        title: 'Conversation snapshot',
        description: 'Periodic summary of conversation dynamics.',
        meaning: 'This snapshot shows the balance of speaking time. High overlap may indicate cross-talk; high silence may indicate pauses or connection issues.',
      };

    // Session health
    case 'session.health':
      return {
        title: 'Session health update',
        description: `Health score: ${typeof event.score === 'number' ? (event.score * 100).toFixed(0) : '?'}%, Status: ${event.status || 'unknown'}.`,
        meaning: 'Composite health metric combining all quality indicators. A score below 70% indicates noticeable quality issues.',
      };

    // Timing events
    case 'timing.jitter_spike':
      return {
        title: 'Timing jitter spike',
        description: `Frame timing variation spiked to ${typeof event.jitter_ms === 'number' ? event.jitter_ms.toFixed(0) : '?'}ms.`,
        meaning: 'Packets arrived with irregular timing. This causes stuttering or jerky playback. Usually indicates network congestion or CPU load.',
      };
    case 'timing.clock_drift':
      return {
        title: 'Clock drift detected',
        description: 'Source and destination clocks are running at different rates.',
        meaning: 'Audio/video will gradually desync over time. This is a fundamental timing mismatch between encoder and decoder clocks.',
      };
    case 'timing.lead_jump':
      return {
        title: 'Lead time jump',
        description: 'Sudden change in audio/video timing relationship.',
        meaning: 'An abrupt shift in sync occurred - possibly due to a source restart or network hiccup.',
      };
    case 'timing.report':
      return {
        title: 'Timing report',
        description: 'Periodic timing metrics summary.',
        meaning: 'Regular update of timing statistics for monitoring purposes.',
      };

    // Incident events
    case 'incident.created':
      return {
        title: 'Incident started',
        description: `New incident detected: ${event.pattern || 'unknown pattern'}. Severity: ${event.severity || 'unknown'}.`,
        meaning: 'Multiple related issues have been correlated into an incident. This represents a pattern of problems that may need attention.',
      };
    case 'incident.updated':
      return {
        title: 'Incident updated',
        description: 'Ongoing incident has new information.',
        meaning: 'Additional events have been added to this incident, or its severity has changed.',
      };
    case 'incident.resolved':
      return {
        title: 'Incident resolved',
        description: 'The incident condition has cleared.',
        meaning: 'The issue pattern is no longer active. Stream quality has returned to normal.',
      };

    default:
      return {
        title: eventType.replace(/[._]/g, ' '),
        description: 'Event detected during stream analysis.',
        meaning: 'This event was captured during stream monitoring.',
      };
  }
}

/** Get meaning for speech state */
function getSpeechStateMeaning(state: string | undefined): string {
  switch (state) {
    case 'speaking':
      return 'Active speech is being detected. Audio contains voice content.';
    case 'silent':
      return 'Brief pause in speech. Normal conversational behavior.';
    case 'dead_air':
      return 'Extended period with no speech. May indicate a problem or intentional break.';
    case 'speaking_left':
      return 'Speech detected on left channel only. Right channel is quiet.';
    case 'speaking_right':
      return 'Speech detected on right channel only. Left channel is quiet.';
    case 'speaking_both':
      return 'Both channels have active speech. Normal for stereo conversation.';
    case 'overlap':
      return 'Multiple speakers talking simultaneously. May indicate cross-talk or interruption.';
    default:
      return 'Speech detection state update.';
  }
}

/** Get title for event (legacy helper) */
function getEventTitle(event: Record<string, unknown>): string {
  return getEventMeta(event).title;
}

/** Render event-specific details */
function renderEventDetails(event: Record<string, unknown>) {
  const eventType = event.event_type as string;
  const rows: Array<{ label: string; value: string }> = [];

  // Add event-type specific details
  if (eventType === 'audio.silence') {
    if (typeof event.duration_ms === 'number') {
      rows.push({ label: 'Duration', value: `${(event.duration_ms / 1000).toFixed(1)} seconds` });
    }
    if (typeof event.threshold_db === 'number') {
      rows.push({ label: 'Threshold', value: `${event.threshold_db} dB` });
    }
  }

  if (eventType === 'audio.clipping') {
    if (typeof event.peak_db === 'number') {
      rows.push({ label: 'Peak level', value: `${event.peak_db.toFixed(1)} dBFS` });
    }
    if (typeof event.clipping_frames === 'number') {
      rows.push({ label: 'Clipping frames', value: event.clipping_frames.toString() });
    }
    if (event.channel) {
      rows.push({ label: 'Channel', value: event.channel as string });
    }
  }

  if (eventType === 'audio.channel_imbalance') {
    if (typeof event.left_db === 'number') {
      rows.push({ label: 'Left RMS', value: `${event.left_db.toFixed(1)} dB` });
    }
    if (typeof event.right_db === 'number') {
      rows.push({ label: 'Right RMS', value: `${event.right_db.toFixed(1)} dB` });
    }
    if (typeof event.imbalance_db === 'number') {
      rows.push({ label: 'Imbalance', value: `${event.imbalance_db.toFixed(1)} dB` });
    }
  }

  if (eventType === 'conversation.flow') {
    if (typeof event.talk_pct === 'number') {
      rows.push({ label: 'Talk time', value: `${(event.talk_pct * 100).toFixed(1)}%` });
    }
    if (typeof event.left_talk_pct === 'number') {
      rows.push({ label: 'Left channel', value: `${(event.left_talk_pct * 100).toFixed(1)}%` });
    }
    if (typeof event.right_talk_pct === 'number') {
      rows.push({ label: 'Right channel', value: `${(event.right_talk_pct * 100).toFixed(1)}%` });
    }
    if (typeof event.overlap_pct === 'number') {
      rows.push({ label: 'Overlap', value: `${(event.overlap_pct * 100).toFixed(1)}%` });
    }
    if (typeof event.silence_pct === 'number') {
      rows.push({ label: 'Silence', value: `${(event.silence_pct * 100).toFixed(1)}%` });
    }
  }

  if (eventType === 'session.health') {
    if (typeof event.score === 'number') {
      rows.push({ label: 'Health score', value: `${(event.score * 100).toFixed(0)}%` });
    }
    if (event.status) {
      rows.push({ label: 'Status', value: event.status as string });
    }
    if (Array.isArray(event.contributors) && event.contributors.length > 0) {
      rows.push({ label: 'Contributors', value: event.contributors.join(', ') });
    }
  }

  if (eventType.startsWith('timing.')) {
    if (typeof event.jitter_ms === 'number') {
      rows.push({ label: 'Jitter', value: `${event.jitter_ms.toFixed(1)} ms` });
    }
    if (typeof event.lead_ms === 'number') {
      rows.push({ label: 'Lead', value: `${event.lead_ms.toFixed(1)} ms` });
    }
    if (typeof event.slope_ms_per_s === 'number') {
      rows.push({ label: 'Drift rate', value: `${event.slope_ms_per_s.toFixed(2)} ms/s` });
    }
    if (typeof event.threshold_ms === 'number') {
      rows.push({ label: 'Threshold', value: `${event.threshold_ms} ms` });
    }
  }

  if (eventType.startsWith('speech.')) {
    if (event.state) {
      rows.push({ label: 'State', value: event.state as string });
    }
    if (event.prev_state) {
      rows.push({ label: 'Previous state', value: event.prev_state as string });
    }
    if (typeof event.duration_ms === 'number') {
      rows.push({ label: 'Duration', value: `${(event.duration_ms / 1000).toFixed(1)} seconds` });
    }
    if (event.channel) {
      rows.push({ label: 'Channel', value: event.channel as string });
    }
  }

  // Drift events from HealthEmitterNode
  if (eventType === 'drift') {
    if (typeof event.lead_ms === 'number') {
      rows.push({ label: 'Current lead', value: `${event.lead_ms} ms` });
    }
    if (typeof event.threshold_ms === 'number') {
      rows.push({ label: 'Threshold', value: `${event.threshold_ms} ms` });
    }
    if (event.stream_id) {
      rows.push({ label: 'Stream', value: event.stream_id as string });
    }
  }

  if (eventType === 'av_skew') {
    if (typeof event.skew_ms === 'number') {
      rows.push({ label: 'A/V skew', value: `${event.skew_ms} ms` });
    }
    if (typeof event.threshold_ms === 'number') {
      rows.push({ label: 'Threshold', value: `${event.threshold_ms} ms` });
    }
  }

  if (eventType === 'cadence') {
    if (typeof event.cv === 'number') {
      rows.push({ label: 'Variation (CV)', value: `${(event.cv * 100).toFixed(1)}%` });
    }
    if (typeof event.threshold === 'number') {
      rows.push({ label: 'Threshold', value: `${(event.threshold * 100).toFixed(0)}%` });
    }
  }

  if (eventType === 'freeze') {
    if (typeof event.duration_ms === 'number') {
      rows.push({ label: 'Freeze duration', value: `${(event.duration_ms / 1000).toFixed(1)} seconds` });
    }
    if (event.stream_id) {
      rows.push({ label: 'Stream', value: event.stream_id as string });
    }
  }

  if (eventType === 'health') {
    if (typeof event.score === 'number') {
      rows.push({ label: 'Health score', value: `${(event.score * 100).toFixed(0)}%` });
    }
    if (Array.isArray(event.alerts) && event.alerts.length > 0) {
      rows.push({ label: 'Active alerts', value: event.alerts.join(', ') });
    }
  }

  // Audio events from HealthEmitterNode
  if (eventType === 'silence') {
    if (typeof event.duration_ms === 'number') {
      rows.push({ label: 'Duration', value: `${(event.duration_ms / 1000).toFixed(1)} seconds` });
    }
    if (typeof event.rms_db === 'number') {
      rows.push({ label: 'Audio level', value: `${event.rms_db.toFixed(0)} dB` });
    }
  }

  if (eventType === 'clipping') {
    if (typeof event.saturation_ratio === 'number') {
      rows.push({ label: 'Saturation', value: `${(event.saturation_ratio * 100).toFixed(1)}%` });
    }
    if (typeof event.crest_factor_db === 'number') {
      rows.push({ label: 'Crest factor', value: `${event.crest_factor_db.toFixed(1)} dB` });
    }
  }

  if (eventType === 'low_volume') {
    if (typeof event.rms_db === 'number') {
      rows.push({ label: 'RMS level', value: `${event.rms_db.toFixed(0)} dB` });
    }
    if (typeof event.peak_db === 'number') {
      rows.push({ label: 'Peak level', value: `${event.peak_db.toFixed(0)} dB` });
    }
  }

  if (eventType === 'channel_imbalance') {
    if (typeof event.imbalance_db === 'number') {
      rows.push({ label: 'Imbalance', value: `${event.imbalance_db.toFixed(0)} dB` });
    }
    if (event.dead_channel && event.dead_channel !== 'none') {
      rows.push({ label: 'Dead channel', value: event.dead_channel as string });
    }
  }

  if (eventType === 'dropouts') {
    if (typeof event.dropout_count === 'number') {
      rows.push({ label: 'Dropout count', value: event.dropout_count.toString() });
    }
  }

  // System events
  if (eventType === 'stream_started') {
    if (event.session_id) {
      rows.push({ label: 'Session ID', value: event.session_id as string });
    }
  }

  if (eventType === 'stream_ended') {
    if (event.reason) {
      rows.push({ label: 'Reason', value: event.reason as string });
    }
    if (typeof event.relative_ms === 'number') {
      rows.push({ label: 'Stream duration', value: `${(event.relative_ms / 1000).toFixed(1)} seconds` });
    }
    if (event.session_id) {
      rows.push({ label: 'Session ID', value: event.session_id as string });
    }
  }

  // Incident events
  if (eventType.startsWith('incident.')) {
    if (event.incident_id) {
      rows.push({ label: 'Incident ID', value: (event.incident_id as string).slice(0, 8) });
    }
    if (event.severity) {
      rows.push({ label: 'Severity', value: event.severity as string });
    }
    if (event.pattern) {
      rows.push({ label: 'Pattern', value: event.pattern as string });
    }
    if (typeof event.event_count === 'number') {
      rows.push({ label: 'Event count', value: event.event_count.toString() });
    }
  }

  // Render rows
  return rows.map(({ label, value }) => (
    <div key={label} className="flex justify-between items-baseline py-1.5 border-b border-surface-elevated last:border-0">
      <span className="text-text-secondary text-sm">{label}</span>
      <span className="text-text-primary font-mono text-sm">{value}</span>
    </div>
  ));
}

/** Render contextual information - likely causes */
function renderEventContext(event: Record<string, unknown>) {
  const eventType = event.event_type as string;
  const hints: string[] = [];

  // Audio quality issues
  if (eventType === 'audio.clipping' || eventType === 'clipping') {
    hints.push('Microphone gain is set too high');
    hints.push('Speaker is too close to the microphone');
    hints.push('Input signal is being overdriven');
  }

  if (eventType === 'audio.channel_imbalance' || eventType === 'channel_imbalance') {
    hints.push('Mono microphone connected to stereo input');
    hints.push('Loose or damaged audio cable');
    hints.push('Incorrect audio routing configuration');
  }

  if (eventType === 'audio.silence' || eventType === 'silence') {
    hints.push('Microphone is muted');
    hints.push('Audio source disconnected');
    hints.push('Incorrect input device selected');
  }

  if (eventType === 'audio.low_volume' || eventType === 'low_volume') {
    hints.push('Microphone gain is too low');
    hints.push('Speaker is too far from microphone');
    hints.push('Input level needs adjustment');
  }

  if (eventType === 'dropouts') {
    hints.push('Network packet loss');
    hints.push('Audio buffer underrun');
    hints.push('CPU unable to keep up with encoding');
  }

  // Drift and timing issues
  if (eventType === 'drift') {
    hints.push('Audio and video sources use different clocks');
    hints.push('Processing delays are accumulating');
    hints.push('Encoder/decoder clock mismatch');
  }

  if (eventType === 'av_skew') {
    hints.push('Encoding pipeline has latency difference');
    hints.push('Network jitter affecting sync');
    hints.push('Source devices have timing offset');
  }

  if (eventType === 'cadence') {
    hints.push('CPU overload causing frame drops');
    hints.push('Network congestion causing packet bunching');
    hints.push('Encoder struggling with content complexity');
  }

  if (eventType === 'freeze') {
    hints.push('Network connection interrupted');
    hints.push('Encoder crashed or stalled');
    hints.push('Source video feed stopped');
  }

  if (eventType === 'timing.jitter_spike' || eventType === 'timing.clock_drift') {
    hints.push('Network path congestion');
    hints.push('CPU throttling or high load');
    hints.push('Buffer configuration issues');
  }

  // Speech events
  if (eventType === 'speech.state' && event.state === 'dead_air') {
    hints.push('Extended pause in conversation');
    hints.push('Participants may have stepped away');
    hints.push('Possible connection issue');
  }

  if (eventType === 'speech.state' && event.state === 'overlap') {
    hints.push('Multiple speakers talking simultaneously');
    hints.push('Cross-talk or interruption');
    hints.push('May indicate animated discussion');
  }

  // System events
  if (eventType === 'stream_ended') {
    const reason = event.reason as string;
    if (reason === 'client_disconnect') {
      hints.push('Client closed the connection normally');
    } else if (reason?.includes('error') || reason?.includes('timeout')) {
      hints.push('Connection was lost unexpectedly');
      hints.push('Check network connectivity');
    }
  }

  if (hints.length === 0) return null;

  return (
    <section>
      <h3 className="text-xs font-medium text-text-muted uppercase tracking-wider mb-3">
        Likely causes
      </h3>
      <ul className="space-y-1.5">
        {hints.map((hint) => (
          <li key={hint} className="flex items-start gap-2 text-sm text-text-secondary">
            <span className="text-text-muted">•</span>
            <span>{hint}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
