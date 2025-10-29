/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,

  // Enable experimental features if needed
  experimental: {
    // Add experimental features here
  },

  // Webpack configuration for handling gRPC
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
