import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import { AnalyticsBootstrap } from "@/components/analytics-bootstrap";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Otell: Local OpenTelemetry Tool Designed for LLM Agents",
  description: "Otell is a local OpenTelemetry tool designed for LLM agents. It allows you to ingest and query telemetry data locally, without the need for a remote collector.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className={`${geistSans.variable} ${geistMono.variable}`}>
        <AnalyticsBootstrap />
        {children}
      </body>
    </html>
  );
}
