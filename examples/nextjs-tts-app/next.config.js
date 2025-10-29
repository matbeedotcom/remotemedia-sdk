/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,

  // Turbopack configuration (Next.js 16+ default bundler)
  // Empty config to silence the warning about webpack config
  turbopack: {},

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
