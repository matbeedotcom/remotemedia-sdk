/**
 * TTS Pipeline Manifest Builder
 *
 * Utilities for building RemoteMedia pipeline manifests for TTS operations.
 * Based on contracts/tts-streaming-protocol.md
 */

import type { VoiceConfig, TTSRequest } from '@/types/tts';

/**
 * Pipeline manifest structure (compatible with RemoteMedia gRPC service)
 */
export interface PipelineManifest {
  version: string;
  metadata: {
    name: string;
    description?: string;
    createdAt?: string;
  };
  nodes: Array<{
    id: string;
    nodeType: string;
    params: string;
    isStreaming?: boolean;
  }>;
  connections?: Array<{
    from: string;
    to: string;
  }>;
}

/**
 * Kokoro TTS Node parameters
 */
export interface KokoroTTSParams {
  text: string;
  language?: string;
  voice?: string;
  speed?: number;
  outputFormat?: 'pcm_f32le' | 'pcm_s16le';
}

/**
 * Pipeline builder options
 */
export interface PipelineBuilderOptions {
  /** Enable streaming mode (default: true) */
  streaming?: boolean;
  /** Pipeline description */
  description?: string;
  /** Additional metadata */
  metadata?: Record<string, string>;
}

/**
 * Build a TTS pipeline manifest from a TTS request
 *
 * @param request - TTS request with text and voice config
 * @param options - Optional pipeline configuration
 * @returns Pipeline manifest ready for gRPC execution
 */
export function buildTTSPipelineFromRequest(
  request: TTSRequest,
  options: PipelineBuilderOptions = {}
): PipelineManifest {
  return buildTTSPipeline(
    request.text,
    request.voiceConfig,
    options
  );
}

/**
 * Build a TTS pipeline manifest
 *
 * @param text - Text to synthesize
 * @param voiceConfig - Voice configuration
 * @param options - Optional pipeline configuration
 * @returns Pipeline manifest ready for gRPC execution
 */
export function buildTTSPipeline(
  text: string,
  voiceConfig: VoiceConfig,
  options: PipelineBuilderOptions = {}
): PipelineManifest {
  const streaming = options.streaming ?? true;

  // Build Kokoro TTS node parameters
  const ttsParams: KokoroTTSParams = {
    text,
    language: voiceConfig.language,
    voice: voiceConfig.voice,
    speed: voiceConfig.speed,
    outputFormat: 'pcm_f32le', // Kokoro outputs Float32 PCM
  };

  // Create pipeline manifest
  const manifest: PipelineManifest = {
    version: 'v1',
    metadata: {
      name: options.description || 'tts-streaming',
      description: options.description || `TTS: ${text.substring(0, 50)}${text.length > 50 ? '...' : ''}`,
      createdAt: new Date().toISOString(),
      ...options.metadata,
    },
    nodes: [
      {
        id: 'tts',
        nodeType: 'KokoroTTSNode',
        params: JSON.stringify(ttsParams),
        isStreaming: streaming,
      },
    ],
    connections: [],
  };

  return manifest;
}

/**
 * Build a multi-node TTS pipeline (future: with effects, processing, etc.)
 *
 * @param text - Text to synthesize
 * @param voiceConfig - Voice configuration
 * @param processingNodes - Additional processing nodes to add
 * @param options - Optional pipeline configuration
 * @returns Pipeline manifest with processing chain
 */
export function buildAdvancedTTSPipeline(
  text: string,
  voiceConfig: VoiceConfig,
  processingNodes: Array<{
    id: string;
    nodeType: string;
    params: Record<string, unknown>;
  }> = [],
  options: PipelineBuilderOptions = {}
): PipelineManifest {
  const streaming = options.streaming ?? true;

  // Start with TTS node
  const ttsParams: KokoroTTSParams = {
    text,
    language: voiceConfig.language,
    voice: voiceConfig.voice,
    speed: voiceConfig.speed,
    outputFormat: 'pcm_f32le',
  };

  const nodes = [
    {
      id: 'tts',
      nodeType: 'KokoroTTSNode',
      params: JSON.stringify(ttsParams),
      isStreaming: streaming,
    },
  ];

  // Add processing nodes
  const connections: Array<{ from: string; to: string }> = [];
  let previousNodeId = 'tts';

  for (const processingNode of processingNodes) {
    nodes.push({
      id: processingNode.id,
      nodeType: processingNode.nodeType,
      params: JSON.stringify(processingNode.params),
      isStreaming: streaming,
    });

    // Connect previous node to this node
    connections.push({
      from: previousNodeId,
      to: processingNode.id,
    });

    previousNodeId = processingNode.id;
  }

  // Create pipeline manifest
  const manifest: PipelineManifest = {
    version: 'v1',
    metadata: {
      name: options.description || 'tts-streaming-advanced',
      description: options.description || `Advanced TTS pipeline with ${processingNodes.length} processing nodes`,
      createdAt: new Date().toISOString(),
      ...options.metadata,
    },
    nodes,
    connections,
  };

  return manifest;
}

/**
 * Validate a pipeline manifest
 *
 * @param manifest - Pipeline manifest to validate
 * @returns Array of validation errors (empty if valid)
 */
export function validatePipelineManifest(manifest: PipelineManifest): string[] {
  const errors: string[] = [];

  // Check version
  if (!manifest.version) {
    errors.push('Pipeline version is required');
  }

  // Check metadata
  if (!manifest.metadata || !manifest.metadata.name) {
    errors.push('Pipeline metadata.name is required');
  }

  // Check nodes
  if (!manifest.nodes || manifest.nodes.length === 0) {
    errors.push('Pipeline must have at least one node');
  }

  // Validate each node
  if (manifest.nodes) {
    manifest.nodes.forEach((node, index) => {
      if (!node.id) {
        errors.push(`Node ${index} is missing id`);
      }
      if (!node.nodeType) {
        errors.push(`Node ${index} (${node.id}) is missing nodeType`);
      }
      if (!node.params) {
        errors.push(`Node ${index} (${node.id}) is missing params`);
      } else {
        // Validate params is valid JSON
        try {
          JSON.parse(node.params);
        } catch (e) {
          errors.push(`Node ${index} (${node.id}) has invalid JSON params: ${e}`);
        }
      }
    });
  }

  // Validate connections
  if (manifest.connections) {
    const nodeIds = new Set(manifest.nodes.map(n => n.id));

    manifest.connections.forEach((conn, index) => {
      if (!conn.from) {
        errors.push(`Connection ${index} is missing 'from' field`);
      } else if (!nodeIds.has(conn.from)) {
        errors.push(`Connection ${index} references non-existent 'from' node: ${conn.from}`);
      }

      if (!conn.to) {
        errors.push(`Connection ${index} is missing 'to' field`);
      } else if (!nodeIds.has(conn.to)) {
        errors.push(`Connection ${index} references non-existent 'to' node: ${conn.to}`);
      }
    });
  }

  return errors;
}

/**
 * Extract TTS parameters from a pipeline manifest
 *
 * @param manifest - Pipeline manifest
 * @returns TTS parameters if found, null otherwise
 */
export function extractTTSParams(manifest: PipelineManifest): KokoroTTSParams | null {
  const ttsNode = manifest.nodes.find(
    node => node.nodeType === 'KokoroTTSNode'
  );

  if (!ttsNode) {
    return null;
  }

  try {
    return JSON.parse(ttsNode.params) as KokoroTTSParams;
  } catch (e) {
    console.error('Failed to parse TTS params:', e);
    return null;
  }
}

/**
 * Clone and update TTS parameters in a pipeline manifest
 *
 * @param manifest - Original pipeline manifest
 * @param updates - TTS parameter updates
 * @returns New manifest with updated parameters
 */
export function updateTTSParams(
  manifest: PipelineManifest,
  updates: Partial<KokoroTTSParams>
): PipelineManifest {
  const newManifest = structuredClone(manifest);

  const ttsNodeIndex = newManifest.nodes.findIndex(
    node => node.nodeType === 'KokoroTTSNode'
  );

  if (ttsNodeIndex === -1) {
    throw new Error('No KokoroTTSNode found in pipeline');
  }

  // Parse existing params
  const existingParams = JSON.parse(newManifest.nodes[ttsNodeIndex].params);

  // Merge with updates
  const updatedParams = {
    ...existingParams,
    ...updates,
  };

  // Update node params
  newManifest.nodes[ttsNodeIndex].params = JSON.stringify(updatedParams);

  return newManifest;
}

/**
 * Get pipeline summary for logging/debugging
 *
 * @param manifest - Pipeline manifest
 * @returns Human-readable summary
 */
export function getPipelineSummary(manifest: PipelineManifest): string {
  const nodeTypes = manifest.nodes.map(n => n.nodeType).join(' → ');
  const streaming = manifest.nodes.some(n => n.isStreaming);

  return `Pipeline "${manifest.metadata.name}": ${nodeTypes} (streaming: ${streaming})`;
}
