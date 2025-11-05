/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,

  // TypeScript configuration
  typescript: {
    // Allow production builds to complete even with type errors
    ignoreBuildErrors: true,
  },

  // Turbopack configuration (Next.js 16+ default bundler)
  // Set the correct workspace root to avoid module resolution issues
  turbopack: {
    root: __dirname, // Use the nextjs-tts-app directory as the root
  },

  // Webpack configuration for handling gRPC (fallback for webpack builds)
  webpack: (config, { isServer }) => {
    // Add fallbacks for Node.js modules not available in browser
    if (!isServer) {
      config.resolve.fallback = {
        ...config.resolve.fallback,
        fs: false,
        net: false,
        tls: false,
        dns: false,
        child_process: false,
      };
    }

    return config;
  },

  // Environment variables validation
  env: {
    NEXT_PUBLIC_GRPC_HOST: process.env.NEXT_PUBLIC_GRPC_HOST || 'localhost',
    NEXT_PUBLIC_GRPC_PORT: process.env.NEXT_PUBLIC_GRPC_PORT || '50051',
  },
};

module.exports = nextConfig;
