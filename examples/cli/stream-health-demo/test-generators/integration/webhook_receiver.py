#!/usr/bin/env python3
"""
Simple webhook receiver for integration testing.

Starts an HTTP server that:
- Receives POST webhooks at /webhook
- Records all received events
- Provides /events endpoint to retrieve recorded events
- Provides /clear endpoint to reset events
- Provides /health endpoint for readiness checks

Usage:
    # Start server
    python webhook_receiver.py --port 8765
    
    # Get recorded events (from another terminal)
    curl http://localhost:8765/events
    
    # Clear events
    curl -X POST http://localhost:8765/clear
"""

import argparse
import json
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from datetime import datetime
from typing import List, Dict


class WebhookStore:
    """Thread-safe store for received webhooks"""
    
    def __init__(self):
        self._events: List[Dict] = []
        self._lock = threading.Lock()
    
    def add(self, event: Dict):
        with self._lock:
            event["_received_at"] = datetime.utcnow().isoformat() + "Z"
            self._events.append(event)
    
    def get_all(self) -> List[Dict]:
        with self._lock:
            return self._events.copy()
    
    def clear(self):
        with self._lock:
            self._events.clear()
    
    def count(self) -> int:
        with self._lock:
            return len(self._events)
    
    def count_by_type(self, event_type: str) -> int:
        with self._lock:
            return sum(1 for e in self._events if e.get("type") == event_type)


# Global store
store = WebhookStore()


class WebhookHandler(BaseHTTPRequestHandler):
    """HTTP handler for webhook endpoints"""
    
    def log_message(self, format, *args):
        """Override to add timestamp"""
        print(f"[{datetime.utcnow().isoformat()}] {args[0]}")
    
    def send_json(self, data: dict, status: int = 200):
        """Send JSON response"""
        body = json.dumps(data, indent=2).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", len(body))
        self.end_headers()
        self.wfile.write(body)
    
    def do_GET(self):
        """Handle GET requests"""
        if self.path == "/health":
            self.send_json({"status": "ok", "event_count": store.count()})
        
        elif self.path == "/events":
            events = store.get_all()
            self.send_json({
                "count": len(events),
                "events": events
            })
        
        elif self.path == "/summary":
            events = store.get_all()
            by_type = {}
            for e in events:
                t = e.get("type", "unknown")
                by_type[t] = by_type.get(t, 0) + 1
            self.send_json({
                "total": len(events),
                "by_type": by_type
            })
        
        else:
            self.send_json({"error": "Not found"}, 404)
    
    def do_POST(self):
        """Handle POST requests"""
        if self.path == "/webhook":
            # Read body
            content_length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(content_length)
            
            try:
                event = json.loads(body)
                store.add(event)
                print(f"  Received: {event.get('type', 'unknown')}")
                self.send_json({"status": "received"})
            except json.JSONDecodeError as e:
                self.send_json({"error": f"Invalid JSON: {e}"}, 400)
        
        elif self.path == "/clear":
            store.clear()
            self.send_json({"status": "cleared"})
        
        else:
            self.send_json({"error": "Not found"}, 404)


def main():
    parser = argparse.ArgumentParser(description="Webhook receiver for integration tests")
    parser.add_argument("--port", "-p", type=int, default=8765, help="Port to listen on")
    parser.add_argument("--host", "-H", type=str, default="127.0.0.1", help="Host to bind")
    args = parser.parse_args()
    
    server = HTTPServer((args.host, args.port), WebhookHandler)
    print(f"Webhook receiver listening on http://{args.host}:{args.port}")
    print(f"  POST /webhook  - Receive events")
    print(f"  GET  /events   - List all events")
    print(f"  GET  /summary  - Event counts by type")
    print(f"  POST /clear    - Clear events")
    print(f"  GET  /health   - Health check")
    print()
    
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...")
        server.shutdown()


if __name__ == "__main__":
    main()
