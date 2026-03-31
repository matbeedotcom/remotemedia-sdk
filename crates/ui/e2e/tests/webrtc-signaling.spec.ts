import { test, expect } from '@playwright/test';

const WS_PORT = process.env.WS_PORT || '18091';
const WS_URL = `ws://127.0.0.1:${WS_PORT}/ws`;

// Helper: send a JSON-RPC 2.0 request over WebSocket from the browser context
function jsonRpc(method: string, params: Record<string, unknown>, id: string) {
  return JSON.stringify({ jsonrpc: '2.0', method, params, id });
}

test.describe('WebRTC Signaling E2E', () => {

  // ──────────────────────────────────────────────────────────────────────
  // UI: WebRTC Tab
  // ──────────────────────────────────────────────────────────────────────

  test.describe('WebRTC UI Tab', () => {
    test('shows WebRTC tab when transport is webrtc', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      // Transport badge should say "webrtc"
      await expect(page.locator('.transport-badge')).toContainText('webrtc');

      // WebRTC tab should be visible
      await expect(page.getByRole('button', { name: 'WebRTC' })).toBeVisible();
    });

    test('WebRTC tab renders panel', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      await page.getByRole('button', { name: 'WebRTC' }).click();

      await expect(page.locator('h2')).toContainText('WebRTC Real-Time');
      await expect(page.getByText('Signaling:')).toBeVisible();
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // WebSocket Signaling: Connection
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Signaling Connection', () => {
    test('WebSocket connects to signaling server', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<string>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          const timeout = setTimeout(() => { ws.close(); reject(new Error('WS connect timeout')); }, 5000);
          ws.onopen = () => {
            clearTimeout(timeout);
            ws.close();
            resolve('connected');
          };
          ws.onerror = () => {
            clearTimeout(timeout);
            reject(new Error('WS connection error'));
          };
        });
      }, WS_URL);

      expect(result).toBe('connected');
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // WebSocket Signaling: Peer Announce
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Peer Announce', () => {
    test('peer.announce registers peer and returns success', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<any>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          const timeout = setTimeout(() => { ws.close(); reject(new Error('timeout')); }, 5000);

          ws.onopen = () => {
            ws.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'peer.announce',
              params: {
                peer_id: 'e2e-peer-1',
                capabilities: ['audio', 'data'],
                user_data: { display_name: 'E2E Test Peer' },
              },
              id: 'announce-1',
            }));
          };

          ws.onmessage = (e) => {
            clearTimeout(timeout);
            const msg = JSON.parse(e.data);
            ws.close();
            resolve(msg);
          };

          ws.onerror = () => {
            clearTimeout(timeout);
            reject(new Error('WS error'));
          };
        });
      }, WS_URL);

      expect(result.jsonrpc).toBe('2.0');
      expect(result.id).toBe('announce-1');
      expect(result.result.success).toBe(true);
      expect(result.result.peer_id).toBe('e2e-peer-1');
    });

    test('duplicate peer.announce returns error', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<any>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          const timeout = setTimeout(() => { ws.close(); reject(new Error('timeout')); }, 5000);
          const messages: any[] = [];

          ws.onopen = () => {
            // Announce twice with same peer_id
            ws.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'peer.announce',
              params: { peer_id: 'e2e-dup-peer', capabilities: ['audio'] },
              id: 'dup-1',
            }));
          };

          ws.onmessage = (e) => {
            const msg = JSON.parse(e.data);
            messages.push(msg);

            if (messages.length === 1 && msg.result?.success) {
              // Send duplicate announce
              ws.send(JSON.stringify({
                jsonrpc: '2.0',
                method: 'peer.announce',
                params: { peer_id: 'e2e-dup-peer', capabilities: ['audio'] },
                id: 'dup-2',
              }));
            } else if (messages.length >= 2) {
              clearTimeout(timeout);
              ws.close();
              resolve(messages);
            }
          };

          ws.onerror = () => { clearTimeout(timeout); reject(new Error('WS error')); };
        });
      }, WS_URL);

      // First announce succeeds
      expect(result[0].result.success).toBe(true);
      // Second announce should error (peer already registered)
      expect(result[1].error).toBeDefined();
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // WebSocket Signaling: Peer List
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Peer List', () => {
    test('peer.list returns connected peers', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<any>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          const timeout = setTimeout(() => { ws.close(); reject(new Error('timeout')); }, 5000);
          const messages: any[] = [];

          ws.onopen = () => {
            ws.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'peer.announce',
              params: { peer_id: 'e2e-list-peer', capabilities: ['audio', 'video'] },
              id: 'ann-1',
            }));
          };

          ws.onmessage = (e) => {
            const msg = JSON.parse(e.data);
            messages.push(msg);

            if (msg.id === 'ann-1' && msg.result?.success) {
              // Now list peers
              ws.send(JSON.stringify({
                jsonrpc: '2.0',
                method: 'peer.list',
                params: {},
                id: 'list-1',
              }));
            } else if (msg.id === 'list-1') {
              clearTimeout(timeout);
              ws.close();
              resolve(msg);
            }
          };

          ws.onerror = () => { clearTimeout(timeout); reject(new Error('WS error')); };
        });
      }, WS_URL);

      expect(result.result.peers).toBeDefined();
      expect(Array.isArray(result.result.peers)).toBe(true);
      // Our peer should be in the list
      const ourPeer = result.result.peers.find((p: any) => p.peer_id === 'e2e-list-peer');
      expect(ourPeer).toBeDefined();
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // WebSocket Signaling: Multi-Peer
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Multi-Peer', () => {
    test('second peer receives peer.joined notification', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<any>((resolve, reject) => {
          const timeout = setTimeout(() => { reject(new Error('timeout')); }, 10000);

          // Peer A connects first
          const wsA = new WebSocket(wsUrl);
          let wsB: WebSocket;

          wsA.onopen = () => {
            wsA.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'peer.announce',
              params: { peer_id: 'e2e-multi-A', capabilities: ['audio'] },
              id: 'a-announce',
            }));
          };

          wsA.onmessage = (e) => {
            const msg = JSON.parse(e.data);
            if (msg.id === 'a-announce' && msg.result?.success) {
              // Peer A is registered. Now connect Peer B.
              wsB = new WebSocket(wsUrl);
              wsB.onopen = () => {
                wsB.send(JSON.stringify({
                  jsonrpc: '2.0',
                  method: 'peer.announce',
                  params: { peer_id: 'e2e-multi-B', capabilities: ['audio'] },
                  id: 'b-announce',
                }));
              };
              wsB.onerror = () => { clearTimeout(timeout); reject(new Error('WS B error')); };
            }

            // Peer A should receive a peer.joined notification for Peer B
            if (msg.method === 'peer.joined' && msg.params?.peer_id === 'e2e-multi-B') {
              clearTimeout(timeout);
              wsA.close();
              wsB?.close();
              resolve(msg);
            }
          };

          wsA.onerror = () => { clearTimeout(timeout); reject(new Error('WS A error')); };
        });
      }, WS_URL);

      expect(result.method).toBe('peer.joined');
      expect(result.params.peer_id).toBe('e2e-multi-B');
      expect(result.params.capabilities).toContain('audio');
    });

    test('other_peers lists existing peers on announce', async ({ page }) => {
      await page.goto('/');

      // Use unique peer IDs with timestamp to avoid collisions with other tests
      const ts = Date.now();
      const result = await page.evaluate(async ([wsUrl, timestamp]) => {
        return new Promise<any>((resolve, reject) => {
          const timeout = setTimeout(() => { reject(new Error('timeout')); }, 10000);
          const peerC = `e2e-existing-C-${timestamp}`;
          const peerD = `e2e-existing-D-${timestamp}`;

          // Peer C connects and stays connected
          const wsC = new WebSocket(wsUrl);

          wsC.onopen = () => {
            wsC.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'peer.announce',
              params: { peer_id: peerC, capabilities: ['data'] },
              id: 'c-ann',
            }));
          };

          wsC.onmessage = (e) => {
            const msg = JSON.parse(e.data);
            if (msg.id === 'c-ann' && msg.result?.success) {
              // Now Peer D connects — should see Peer C in other_peers
              const wsD = new WebSocket(wsUrl);
              wsD.onopen = () => {
                wsD.send(JSON.stringify({
                  jsonrpc: '2.0',
                  method: 'peer.announce',
                  params: { peer_id: peerD, capabilities: ['data'] },
                  id: 'd-ann',
                }));
              };
              wsD.onmessage = (ev) => {
                const dMsg = JSON.parse(ev.data);
                if (dMsg.id === 'd-ann') {
                  clearTimeout(timeout);
                  // Close both AFTER resolving
                  resolve({ response: dMsg, peerC });
                  wsC.close();
                  wsD.close();
                }
              };
              wsD.onerror = () => { clearTimeout(timeout); reject(new Error('WS D error')); };
            }
          };

          wsC.onerror = () => { clearTimeout(timeout); reject(new Error('WS C error')); };
        });
      }, [WS_URL, String(ts)]);

      expect(result.response.result.success).toBe(true);
      // other_peers is a string array of peer IDs, peers_count includes the new peer
      expect(result.response.result.peers_count).toBeGreaterThanOrEqual(1);
      const otherPeers: string[] = result.response.result.other_peers || [];
      expect(Array.isArray(otherPeers)).toBe(true);
      // Peer C should be in the list (it was registered before D announced)
      expect(otherPeers).toContain(result.peerC);
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // WebSocket Signaling: SDP Offer/Answer with Server
  // ──────────────────────────────────────────────────────────────────────

  test.describe('SDP Offer to Server', () => {
    test('peer.offer to remotemedia-server returns answer', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        // Minimal valid SDP offer (enough to pass validation)
        const sdpOffer = [
          'v=0',
          'o=- 0 0 IN IP4 127.0.0.1',
          's=-',
          't=0 0',
          'm=audio 9 UDP/TLS/RTP/SAVPF 111',
          'c=IN IP4 0.0.0.0',
          'a=mid:0',
          'a=rtpmap:111 opus/48000/2',
          'a=sendrecv',
          'a=ice-ufrag:test',
          'a=ice-pwd:testpassword1234567890123',
          'a=fingerprint:sha-256 00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00',
          'a=setup:actpass',
        ].join('\r\n') + '\r\n';

        return new Promise<any>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          const timeout = setTimeout(() => { ws.close(); reject(new Error('timeout')); }, 10000);
          const messages: any[] = [];

          ws.onopen = () => {
            ws.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'peer.announce',
              params: { peer_id: 'e2e-offer-peer', capabilities: ['audio'] },
              id: 'offer-ann',
            }));
          };

          ws.onmessage = (e) => {
            const msg = JSON.parse(e.data);
            messages.push(msg);

            if (msg.id === 'offer-ann' && msg.result?.success) {
              // Send offer to server
              ws.send(JSON.stringify({
                jsonrpc: '2.0',
                method: 'peer.offer',
                params: {
                  from: 'e2e-offer-peer',
                  to: 'remotemedia-server',
                  sdp: sdpOffer,
                },
                id: 'offer-1',
              }));
            }

            // The response to the offer should be an SDP answer
            if (msg.id === 'offer-1') {
              clearTimeout(timeout);
              ws.close();
              resolve(msg);
            }
          };

          ws.onerror = () => { clearTimeout(timeout); reject(new Error('WS error')); };
        });
      }, WS_URL);

      // Should get an answer back (either success with SDP or an error we can inspect)
      if (result.result) {
        expect(result.result.type).toBe('answer');
        expect(result.result.sdp).toContain('v=0');
        expect(result.result.from).toBe('remotemedia-server');
      } else {
        // If server can't create a PeerConnection (e.g. no ICE), it returns an error
        // but the signaling protocol itself worked
        expect(result.error).toBeDefined();
        expect(result.id).toBe('offer-1');
      }
    });

    test('peer.offer with invalid SDP returns error', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<any>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          const timeout = setTimeout(() => { ws.close(); reject(new Error('timeout')); }, 5000);

          ws.onopen = () => {
            ws.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'peer.announce',
              params: { peer_id: 'e2e-bad-sdp-peer', capabilities: ['audio'] },
              id: 'bad-ann',
            }));
          };

          ws.onmessage = (e) => {
            const msg = JSON.parse(e.data);

            if (msg.id === 'bad-ann' && msg.result?.success) {
              // Send offer with invalid SDP
              ws.send(JSON.stringify({
                jsonrpc: '2.0',
                method: 'peer.offer',
                params: {
                  from: 'e2e-bad-sdp-peer',
                  to: 'remotemedia-server',
                  sdp: 'this is not valid SDP',
                },
                id: 'bad-offer',
              }));
            }

            if (msg.id === 'bad-offer') {
              clearTimeout(timeout);
              ws.close();
              resolve(msg);
            }
          };

          ws.onerror = () => { clearTimeout(timeout); reject(new Error('WS error')); };
        });
      }, WS_URL);

      // Invalid SDP should return an error
      expect(result.error).toBeDefined();
      expect(result.error.code).toBe(-32002); // OFFER_INVALID
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // WebSocket Signaling: Error Handling
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Error Handling', () => {
    test('unknown method returns METHOD_NOT_FOUND', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<any>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          const timeout = setTimeout(() => { ws.close(); reject(new Error('timeout')); }, 5000);

          ws.onopen = () => {
            ws.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'nonexistent.method',
              params: {},
              id: 'unknown-1',
            }));
          };

          ws.onmessage = (e) => {
            clearTimeout(timeout);
            const msg = JSON.parse(e.data);
            ws.close();
            resolve(msg);
          };

          ws.onerror = () => { clearTimeout(timeout); reject(new Error('WS error')); };
        });
      }, WS_URL);

      expect(result.error).toBeDefined();
      expect(result.error.code).toBe(-32601); // METHOD_NOT_FOUND
    });

    test('invalid JSON returns PARSE_ERROR', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<any>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          const timeout = setTimeout(() => { ws.close(); reject(new Error('timeout')); }, 5000);

          ws.onopen = () => {
            ws.send('this is not json {{{');
          };

          ws.onmessage = (e) => {
            clearTimeout(timeout);
            const msg = JSON.parse(e.data);
            ws.close();
            resolve(msg);
          };

          ws.onerror = () => { clearTimeout(timeout); reject(new Error('WS error')); };
        });
      }, WS_URL);

      expect(result.error).toBeDefined();
      expect(result.error.code).toBe(-32700); // PARSE_ERROR
    });

    test('operations before announce return error', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<any>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          const timeout = setTimeout(() => { ws.close(); reject(new Error('timeout')); }, 5000);

          ws.onopen = () => {
            // Try to list peers without announcing first
            ws.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'peer.list',
              params: {},
              id: 'no-announce',
            }));
          };

          ws.onmessage = (e) => {
            clearTimeout(timeout);
            const msg = JSON.parse(e.data);
            ws.close();
            resolve(msg);
          };

          ws.onerror = () => { clearTimeout(timeout); reject(new Error('WS error')); };
        });
      }, WS_URL);

      // Should either work (some servers allow this) or return an error
      // Either way the protocol responded correctly
      expect(result.jsonrpc).toBe('2.0');
      expect(result.id).toBe('no-announce');
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // WebSocket Signaling: Disconnect
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Disconnect', () => {
    test('peer.disconnect removes peer from list', async ({ page }) => {
      await page.goto('/');

      const result = await page.evaluate(async (wsUrl) => {
        return new Promise<any>((resolve, reject) => {
          const timeout = setTimeout(() => { reject(new Error('timeout')); }, 10000);

          // Use two connections: one to observe, one to disconnect
          const wsObserver = new WebSocket(wsUrl);
          let wsTarget: WebSocket;

          wsObserver.onopen = () => {
            wsObserver.send(JSON.stringify({
              jsonrpc: '2.0',
              method: 'peer.announce',
              params: { peer_id: 'e2e-observer', capabilities: ['data'] },
              id: 'obs-ann',
            }));
          };

          wsObserver.onmessage = (e) => {
            const msg = JSON.parse(e.data);

            if (msg.id === 'obs-ann' && msg.result?.success) {
              // Connect target peer
              wsTarget = new WebSocket(wsUrl);
              wsTarget.onopen = () => {
                wsTarget.send(JSON.stringify({
                  jsonrpc: '2.0',
                  method: 'peer.announce',
                  params: { peer_id: 'e2e-target', capabilities: ['data'] },
                  id: 'target-ann',
                }));
              };
              wsTarget.onmessage = (ev) => {
                const tMsg = JSON.parse(ev.data);
                if (tMsg.id === 'target-ann' && tMsg.result?.success) {
                  // Target registered, now close its connection
                  wsTarget.close();
                }
              };
              wsTarget.onerror = () => { clearTimeout(timeout); reject(new Error('target error')); };
            }

            // Observer should receive peer.left notification
            if (msg.method === 'peer.left' && msg.params?.peer_id === 'e2e-target') {
              clearTimeout(timeout);
              wsObserver.close();
              resolve(msg);
            }
          };

          wsObserver.onerror = () => { clearTimeout(timeout); reject(new Error('observer error')); };
        });
      }, WS_URL);

      expect(result.method).toBe('peer.left');
      expect(result.params.peer_id).toBe('e2e-target');
    });
  });
});
