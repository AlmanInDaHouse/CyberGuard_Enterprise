import type { Metadata } from "next";
import type { ReactNode } from "react";
import { Providers } from "./providers";
import "./globals.css";

// Every page is rendered at request time, so Next puts the response's CSP nonce on its
// scripts (SPEC-023 §Operational §7): a page prerendered at build would carry none, and
// the CSP would block its scripts.
export const dynamic = "force-dynamic";

export const metadata: Metadata = {
  title: "CyberGuard SOC",
  description: "Minimal SOC dashboard — incidents and alerts (SPEC-009).",
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body className="min-h-screen bg-slate-50 text-slate-900">
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
