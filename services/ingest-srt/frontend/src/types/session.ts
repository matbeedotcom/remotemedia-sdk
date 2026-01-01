/** Session state */
export type SessionStatus = 'idle' | 'created' | 'connecting' | 'streaming' | 'disconnected' | 'ended';

/** Pipeline template */
export interface PipelineTemplate {
  id: string;
  name: string;
  description: string;
  category: 'business' | 'technical' | 'audio' | 'video';
}

/** Available pipelines */
export const PIPELINES: PipelineTemplate[] = [
  {
    id: 'contact_center_qa_v1',
    name: 'Contact Center QA',
    description: 'Speech presence, talk ratios, session health',
    category: 'business',
  },
  {
    id: 'full_stream_health_v1',
    name: 'Full Stream Health',
    description: 'Complete monitoring with business + technical layers',
    category: 'business',
  },
  {
    id: 'technical_stream_analysis_v1',
    name: 'Technical Stream Analysis',
    description: 'Timing drift, event correlation, audio evidence',
    category: 'technical',
  },
  {
    id: 'demo_audio_quality_v1',
    name: 'Audio Quality',
    description: 'Silence, clipping, volume, channel balance',
    category: 'audio',
  },
  {
    id: 'demo_av_quality_v1',
    name: 'A/V Quality',
    description: 'Combined audio and video analysis',
    category: 'audio',
  },
  {
    id: 'demo_video_integrity_v1',
    name: 'Video Integrity',
    description: 'Freeze frames and black frames',
    category: 'video',
  },
];

/** Session creation request */
export interface CreateSessionRequest {
  pipeline: string;
  webhook_url?: string;
  audio_enabled: boolean;
  video_enabled: boolean;
  max_duration_seconds: number;
}

/** Session creation response */
export interface Session {
  session_id: string;
  srt_url: string;
  expires_at: string;
  ffmpeg_command_copy: string;
  ffmpeg_command_transcode: string;
  webhook_url?: string;
}

/** Gateway metrics */
export interface GatewayMetrics {
  active_sessions: number;
  events_emitted: number;
  uptime_secs: number;
}

/** Session summary for completion state */
export interface SessionSummary {
  /** Duration in milliseconds */
  durationMs: number;
  /** Alert counts by event type */
  alertCounts: Record<string, number>;
  /** Total alerts detected */
  totalAlerts: number;
  /** Whether webhook was configured */
  webhookConfigured: boolean;
}
