import { extractText } from "unpdf";

// SPEC-013 report ACs assert by EXTRACTED CONTENT, never PDF bytes (the @react-pdf byte-output
// is non-deterministic — embedded metadata). `unpdf` is a serverless-friendly pdf.js wrapper
// with no native binaries (devDependency only; the api builds PDFs, never parses them at runtime).

/** All text of a PDF buffer, merged across pages. */
export async function pdfText(buf: Buffer): Promise<string> {
  const { text } = await extractText(new Uint8Array(buf), { mergePages: true });
  return Array.isArray(text) ? text.join("\n") : text;
}

/** Whitespace-collapsed + lowercased — robust for phrase assertions across PDF line reflow. */
export function norm(s: string): string {
  return s.replace(/\s+/g, " ").trim().toLowerCase();
}

/** Whitespace-STRIPPED — recovers long unbroken tokens (the hex / base64url seal) that PDF text
 *  extraction may split across lines, so an exact seal substring match is reliable. */
export function stripWs(s: string): string {
  return s.replace(/\s+/g, "");
}
