// Session Control Bus client — wraps `control.*` JSON-RPC methods and
// surfaces `control.event` notifications as typed callbacks.

import type { SignalingClient } from './signaling'

export type ControlEventKind = 'text' | 'json' | 'audio' | 'binary' | 'other'

export interface ControlEvent {
  topic: string
  kind: ControlEventKind
  payload: unknown
  ts: number
}

export type ControlEventHandler = (ev: ControlEvent) => void

export class ControlBusClient {
  private ws: SignalingClient
  private topics = new Map<string, Set<ControlEventHandler>>()
  private offNotif: (() => void) | null = null

  constructor(ws: SignalingClient) {
    this.ws = ws
    this.offNotif = ws.onNotification((method, params) => {
      if (method !== 'control.event') return
      const p = params as ControlEvent | undefined
      if (!p || typeof p.topic !== 'string') return
      const handlers = this.topics.get(p.topic)
      if (!handlers) return
      for (const h of handlers) {
        try {
          h(p)
        } catch (e) {
          console.error('[control] handler threw for', p.topic, e)
        }
      }
    })
  }

  dispose() {
    this.offNotif?.()
    this.offNotif = null
    this.topics.clear()
  }

  /// Subscribe to a topic (`node.out[.port]`). The returned dispose fn
  /// removes *this* handler only; the server-side subscription stays
  /// active until `unsubscribe` is called explicitly.
  async subscribe(
    topic: string,
    handler: ControlEventHandler,
    opts?: { includeAudio?: boolean },
  ): Promise<() => void> {
    let set = this.topics.get(topic)
    const isFirst = !set
    if (!set) {
      set = new Set()
      this.topics.set(topic, set)
    }
    set.add(handler)
    if (isFirst) {
      await this.ws.call('control.subscribe', {
        topic,
        include_audio: opts?.includeAudio ?? false,
      })
    }
    return () => {
      const s = this.topics.get(topic)
      if (!s) return
      s.delete(handler)
      if (s.size === 0) {
        this.topics.delete(topic)
        this.ws
          .call('control.unsubscribe', { topic })
          .catch((e) =>
            console.warn('[control] unsubscribe failed for', topic, e),
          )
      }
    }
  }

  async publishText(topic: string, text: string): Promise<void> {
    await this.ws.call('control.publish', {
      topic,
      payload: { text },
    })
  }

  async publishJson(topic: string, json: unknown): Promise<void> {
    await this.ws.call('control.publish', {
      topic,
      payload: { json },
    })
  }

  async setNodeState(
    nodeId: string,
    state: 'enabled' | 'bypass' | 'disabled',
  ): Promise<void> {
    await this.ws.call('control.set_node_state', { node_id: nodeId, state })
  }

  /// Drain any queued TTS audio from the WebRTC send buffer on the
  /// server. Call this alongside publishing `audio.in.barge_in` so
  /// that the assistant actually stops speaking immediately, not
  /// after the ~10 s of already-generated audio finishes playing.
  async flushAudio(): Promise<{ frames_dropped: number }> {
    return (await this.ws.call('control.flush_audio')) as {
      frames_dropped: number
    }
  }
}
