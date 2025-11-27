# Inference API Dashboard

A web dashboard for testing and monitoring the Inference API.

## Features

- View available pipelines
- Test predictions with file upload or text input
- Streaming playground with real-time output
- Response viewer with timing metrics

## Setup

```bash
# Install dependencies
npm install

# Start development server
npm run dev
```

## Configuration

Create `.env.local` to configure the API URL:

```
VITE_API_URL=http://localhost:8000
```

## Usage

1. Start the API server first
2. Open the dashboard at http://localhost:5173
3. Select a pipeline from the list
4. Use the predict form for unary execution
5. Use the streaming playground for real-time interaction

## Building

```bash
npm run build
```

Output is in `dist/` and can be served statically.

## License

Apache-2.0
