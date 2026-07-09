import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  typedRoutes: true,
  images: {
    remotePatterns: [
      {
        protocol: "https",
        hostname: "cdn.cerul.ai",
      },
    ],
    // Dev-only: local fake-IP DNS proxies (198.18.x.x) trip Next's private-IP
    // SSRF guard when the optimizer fetches cdn.cerul.ai. No effect in prod.
    dangerouslyAllowLocalIP: process.env.NODE_ENV === "development",
  },
  async headers() {
    if (process.env.NODE_ENV === "production") {
      return [];
    }
    return [
      {
        source: "/:path*",
        headers: [
          {
            key: "Referrer-Policy",
            value: "no-referrer-when-downgrade",
          },
        ],
      },
    ];
  },
};

export default nextConfig;
