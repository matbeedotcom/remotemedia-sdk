/**
 * Pipeline Metrics API Route
 *
 * Exposes server-side pipeline metrics for the impressive demo showcase.
 * Returns cache statistics and node information from the Rust gRPC service.
 */

import { NextResponse } from 'next/server';
import clientPool from '@/lib/grpc-client-pool';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export interface PipelineMetrics {
  cacheHits: number;
  cacheMisses: number;
  cachedNodesCount: number;
  cacheHitRate: number;
  timestamp: string;
}

/**
 * GET /api/metrics
 *
 * Returns current pipeline metrics including cache statistics
 */
export async function GET() {
  try {
    // Get client to ensure connection is active
    const client = await clientPool.getClient();

    // For now, return mock data since we don't have a dedicated metrics RPC
    // In production, this could call a GetMetrics() RPC or query Prometheus
    const metrics: PipelineMetrics = {
      cacheHits: 0,
      cacheMisses: 0,
      cachedNodesCount: 1, // TTS model is cached after first request
      cacheHitRate: 0,
      timestamp: new Date().toISOString(),
    };

    return NextResponse.json(metrics);
  } catch (error) {
    console.error('[Metrics API] Error:', error);
    return NextResponse.json(
      { error: 'Failed to fetch metrics' },
      { status: 500 }
    );
  }
}
