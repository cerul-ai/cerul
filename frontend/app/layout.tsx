import type { Metadata } from "next";
import localFont from "next/font/local";
import { GoogleAnalytics } from "@next/third-parties/google";
import {
  defaultOpenGraphImages,
  defaultTwitterImages,
} from "@/lib/social-metadata";
import { getSiteOrigin } from "@/lib/site-url";
import "./globals.css";

const spaceGroteskDisplay = localFont({
  src: [
    {
      path: "./fonts/space-grotesk-400.ttf",
      weight: "400",
      style: "normal",
    },
    {
      path: "./fonts/space-grotesk-500.ttf",
      weight: "500",
      style: "normal",
    },
    {
      path: "./fonts/space-grotesk-700.ttf",
      weight: "700",
      style: "normal",
    },
  ],
  variable: "--font-display",
});

const spaceGroteskSans = localFont({
  src: [
    {
      path: "./fonts/space-grotesk-400.ttf",
      weight: "400",
      style: "normal",
    },
    {
      path: "./fonts/space-grotesk-500.ttf",
      weight: "500",
      style: "normal",
    },
    {
      path: "./fonts/space-grotesk-700.ttf",
      weight: "700",
      style: "normal",
    },
  ],
  variable: "--font-sans",
});

const jetBrainsMono = localFont({
  src: [
    {
      path: "./fonts/jetbrains-mono-400.ttf",
      weight: "400",
      style: "normal",
    },
    {
      path: "./fonts/jetbrains-mono-500.ttf",
      weight: "500",
      style: "normal",
    },
    {
      path: "./fonts/jetbrains-mono-600.ttf",
      weight: "600",
      style: "normal",
    },
  ],
  variable: "--font-mono",
});

const siteOrigin = getSiteOrigin();

export const metadata: Metadata = {
  metadataBase: new URL(siteOrigin),
  title: {
    default: "Cerul",
    template: "%s | Cerul",
  },
  description:
    "Cerul — where video becomes citable. Search video by meaning — across speech, visuals, and on-screen text.",
  icons: {
    icon: [
      { url: "/logo.svg", type: "image/svg+xml" },
      { url: "/favicon.ico", sizes: "48x48" },
      { url: "/icon-192.png", sizes: "192x192", type: "image/png" },
      { url: "/icon-512.png", sizes: "512x512", type: "image/png" },
    ],
    apple: [{ url: "/apple-touch-icon.png", sizes: "180x180" }],
  },
  alternates: {
    canonical: "/",
  },
  openGraph: {
    title: "Cerul",
    description:
      "Cerul — where video becomes citable. Search video by meaning — across speech, visuals, and on-screen text.",
    url: siteOrigin,
    siteName: "Cerul",
    type: "website",
    images: defaultOpenGraphImages,
  },
  twitter: {
    card: "summary_large_image",
    title: "Cerul",
    description:
      "Cerul — where video becomes citable. Search video by meaning — across speech, visuals, and on-screen text.",
    images: defaultTwitterImages,
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${spaceGroteskDisplay.variable} ${spaceGroteskSans.variable} ${jetBrainsMono.variable}`}
    >
      <body suppressHydrationWarning>{children}</body>
      <GoogleAnalytics gaId="G-9316B2EDH4" />
    </html>
  );
}
