# gRPC-Web Integration for Next.js TTS App

This document explains the gRPC-Web integration that allows the Next.js TTS app to communicate directly with the Rust gRPC server from the browser.

## Architecture

```
Browser (Next.js) → gRPC-Web (HTTP/1.1) → Rust gRPC Server
```

The Rust server now supports both:
- **Native gRPC (HTTP/2)**: For server-to-server communication
- **gRPC-Web (HTTP/1.1)**: For browser clients

## Setup

### 1. Generate TypeScript Types from Protos

```bash
cd examples/nextjs-tts-app
npm run generate:protos
```

This generates TypeScript types in `src/generated/` from the proto files in `runtime/protos/`.

### 2. Start the gRPC Server with gRPC-Web Support

The server now includes:
- `tonic-web` for gRPC-Web protocol translation
- `tower-http` CORS middleware for browser requests
- HTTP/1.1 support enabled

```bash
cd runtime
cargo run --release --bin grpc_server --features grpc-transport
```

The server will listen on `http://localhost:50051` and accept both gRPC and gRPC-Web requests.

### 3. Configure Environment Variables

Create `.env.local`:

```bash
# gRPC-Web endpoint
NEXT_PUBLIC_GRPC_HOST=http://localhost:50051
```

### 4. Start the Next.js App

```bash
cd examples/nextjs-tts-app
npm run dev
```

## How It Works

### Client-Side (Browser)

The app uses `@protobuf-ts` packages for gRPC-Web communication:

1. **`TTSGrpcWebClient`** (`src/lib/grpc-web-client.ts`):
   - Browser-compatible gRPC-Web client
   - Uses `GrpcWebFetchTransport` for HTTP/1.1 communication
   - Handles streaming TTS responses

2. **`useTTS` Hook** (`src/hooks/useTTS.ts`):
   - React hook that manages TTS state
   - Uses `TTSGrpcWebClient` instead of Node.js gRPC client
   - Handles audio streaming and playback

### Server-Side (Rust)

The gRPC server (`runtime/src/grpc_service/server.rs`) includes:

1. **gRPC-Web Layer**: `tonic_web::GrpcWebLayer`
   - Translates gRPC-Web (HTTP/1.1) to native gRPC
   - Handles binary and text framing

2. **CORS Layer**: `tower_http::cors::CorsLayer`
   - Allows browser cross-origin requests
   - Configured as permissive for development

3. **HTTP/1.1 Support**: `.accept_http1(true)`
   - Required for gRPC-Web protocol

## Dependencies

### Runtime (Rust)

```toml
tonic = "0.14"
tonic-web = "0.14.1"
tonic-prost = "0.14"
tower-http = { version = "0.5", features = ["cors"] }
```

### Next.js App (TypeScript)

```json
{
  "@protobuf-ts/runtime": "^2.11.1",
  "@protobuf-ts/runtime-rpc": "^2.11.1",
  "@protobuf-ts/grpcweb-transport": "^2.11.1",
  "@protobuf-ts/plugin": "^2.11.1" // dev
}
```

## Protocol Details

### Request Flow

1. **Browser** sends gRPC-Web request (HTTP/1.1 POST)
2. **tonic-web** translates to native gRPC
3. **StreamingPipelineService** processes the request
4. **Response** flows back through tonic-web to browser

### Audio Streaming

```typescript
// Client creates manifest
const manifest = client.createTTSManifest(text, voiceConfig);

// Start streaming with callbacks
await client.startTTSStream(manifest, {
  onReady: () => console.log('Stream ready'),
  onChunk: (chunk) => audioPlayer.addChunk(chunk),
  onComplete: () => console.log('Stream complete'),
  onError: (error) => console.error('Stream error', error),
});
```

### Wire Format

- **Encoding**: Binary protobuf (more efficient than JSON)
- **Framing**: gRPC-Web binary framing
- **Headers**: Standard gRPC metadata (content-type, grpc-status, etc.)

## Troubleshooting

### CORS Errors

If you see CORS errors in the browser console:
- Ensure the server has `.accept_http1(true)`
- Check that `CorsLayer::permissive()` is applied
- Verify the `NEXT_PUBLIC_GRPC_HOST` matches the server address

### Connection Failed

If the client can't connect:
- Verify the gRPC server is running: `cargo run --bin grpc_server`
- Check the server is listening on `localhost:50051`
- Ensure firewall allows connections to port 50051

### Audio Not Playing

If audio chunks arrive but don't play:
- Check browser console for `AudioContext` errors
- Verify audio format conversion in `grpc-web-client.ts`
- Check `AudioPlayer` state in React DevTools

### Type Errors

If you see TypeScript errors about generated types:
- Regenerate types: `npm run generate:protos`
- Check that proto files in `runtime/protos/` are valid
- Ensure `@protobuf-ts/plugin` is installed

## Development vs Production

### Development
- Use `http://localhost:50051` for local testing
- CORS is permissive (allows all origins)
- Server logs are verbose

### Production
- Deploy server with TLS: `https://your-domain.com`
- Configure specific CORS origins
- Update `NEXT_PUBLIC_GRPC_HOST` environment variable
- Consider using a CDN/proxy for gRPC-Web

## Next Steps

- [ ] Add authentication (API keys in metadata)
- [ ] Implement retry logic for failed requests
- [ ] Add request/response interceptors for logging
- [ ] Optimize audio buffering for lower latency
- [ ] Add metrics and monitoring
