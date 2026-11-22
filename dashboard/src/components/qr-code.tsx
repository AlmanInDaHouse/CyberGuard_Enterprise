import { encodeQR } from "qr";

/** The path of a QR symbol's dark modules, one unit square each, quiet zone included, and the
 *  symbol's size in modules. `qr` (0.7.2, MIT OR Apache-2.0, no dependency) encodes it. */
export function qrSymbol(text: string): { size: number; path: string } {
  const modules = encodeQR(text, "raw", { ecc: "medium", border: 4 });
  let path = "";
  modules.forEach((row, y) => {
    row.forEach((dark, x) => {
      if (dark) path += `M${x} ${y}h1v1h-1z`;
    });
  });
  return { size: modules.length, path };
}

/**
 * A QR code drawn as the page's own SVG elements (SPEC-026 §Operational §9): no markup is
 * injected and no image is loaded, so the page's Content Security Policy is unchanged.
 */
export function QrCode({ text, label }: { text: string; label: string }) {
  const { size, path } = qrSymbol(text);
  return (
    <svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${size} ${size}`}
      width={240}
      height={240}
      shapeRendering="crispEdges"
      className="rounded-md border border-slate-300"
    >
      <rect width={size} height={size} fill="#ffffff" />
      <path d={path} fill="#000000" />
    </svg>
  );
}
