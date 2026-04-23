// WebSocket + JSON-RPC 2.0 signaling client for the remotemedia WebRTC
// server. The same connection carries both the WebRTC SDP/ICE exchange
// and the Session Control Bus (`control.*`) — there is only one socket
// per session.

export type JsonRpcId = number | string

export interface JsonRpcRequest {
  jsonrpc: '2.0'
  method: string
  params?: unknown
  id?: JsonRpcId
}

export interface JsonRpcSuccess {
  jsonrpc: '2.0'
  result: unknown
  id: JsonRpcId
}

export interface JsonRpcError {
  jsonrpc: '2.0'
  error: { code: number; message: string; data?: unknown }
  id: JsonRpcId | null
}

export interface JsonRpcNotification {
  jsonrpc: '2.0'
  method: string
  params?: unknown
}

export type NotificationHandler = (method: string, params: unknown) => void

type PendingEntry = {
  resolve: (v: unknown) => void
  reject: (e: Error) => void
}

export class SignalingClient {
  private ws: WebSocket | null = null
  private url: string
  private pending = new Map<JsonRpcId, PendingEntry>()
  private nextId = 1
  private handlers = new Set<NotificationHandler>()
  private openPromise: Promise<void> | null = null
  private onClose?: (ev: CloseEvent) => void

  constructor(url: string) {
    this.url = url
  }

  async connect(): Promise<void> {
    if (this.openPromise) return this.openPromise
    this.openPromise = new Promise((resolve, reject) => {
      const ws = new WebSocket(this.url)
      this.ws = ws
      ws.onopen = () => resolve()
      ws.onerror = () =>
        reject(new Error(`WebSocket error connecting to ${this.url}`))
      ws.onclose = (ev) => {
        this.openPromise = null
        this.ws = null
        for (const [, entry] of this.pending) {
          entry.reject(new Error('WebSocket closed'))
        }
        this.pending.clear()
        this.onClose?.(ev)
      }
      ws.onmessage = (ev) => this.onMessage(ev.data)
    })
    return this.openPromise
  }

  onClosed(cb: (ev: CloseEvent) => void) {
    this.onClose = cb
  }

  isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN
  }

  close() {
    this.ws?.close()
    this.ws = null
    this.openPromise = null
  }

  private onMessage(raw: unknown) {
    if (typeof raw !== 'string') return
    let msg: JsonRpcSuccess | JsonRpcError | JsonRpcNotification
    try {
      msg = JSON.parse(raw)
    } catch (e) {
      console.error('[signaling] bad JSON from server:', raw, e)
      return
    }
    if ('id' in msg && msg.id !== undefined && msg.id !== null) {
      const entry = this.pending.get(msg.id)
      if (!entry) {
        // Notification that happens to carry an id (or a late response
        // for a discarded request). Fall through to notification path.
      } else {
        this.pending.delete(msg.id)
        if ('result' in msg) entry.resolve(msg.result)
        else if ('error' in msg)
          entry.reject(
            new Error(`JSON-RPC ${msg.error.code}: ${msg.error.message}`),
          )
        return
      }
    }
    if ('method' in msg) {
      for (const h of this.handlers) {
        try {
          h(msg.method, msg.params)
        } catch (e) {
          console.error('[signaling] handler threw:', e)
        }
      }
    }
  }

  onNotification(handler: NotificationHandler): () => void {
    this.handlers.add(handler)
    return () => this.handlers.delete(handler)
  }

  async call<T = unknown>(method: string, params?: unknown): Promise<T> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('signaling ws not open')
    }
    const id = this.nextId++
    const req: JsonRpcRequest = { jsonrpc: '2.0', method, params, id }
    const payload = JSON.stringify(req)
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: (v) => resolve(v as T),
        reject,
      })
      this.ws!.send(payload)
    })
  }

  notify(method: string, params?: unknown) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('signaling ws not open')
    }
    const req: JsonRpcRequest = { jsonrpc: '2.0', method, params }
    this.ws.send(JSON.stringify(req))
  }
}
