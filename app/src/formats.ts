// The five formats Phramer saves (crates/phramer-overlay/src/export/encode.rs),
// named by the extension a file of each gets: what the editor and the
// settings window send the daemon.

export type Format = "png" | "jpg" | "bmp" | "webp" | "pdf";

export const FORMATS: readonly { id: Format; label: string }[] = [
  { id: "png", label: "PNG" },
  { id: "jpg", label: "JPEG" },
  { id: "bmp", label: "BMP" },
  { id: "webp", label: "WebP" },
  { id: "pdf", label: "PDF" },
];

/** What is said when another format is asked for. */
export const FORMATS_HINT = "Phramer saves PNG, JPEG, BMP, WebP or PDF";

/** The format of an extension, in any case and with or without its dot
 * (`jpeg` is JPEG); `null` for one Phramer does not save. */
export function formatOf(extension: string): Format | null {
  const plain = extension.replace(/^\./, "").toLowerCase();
  if (plain === "jpeg") return "jpg";
  return FORMATS.some((format) => format.id === plain) ? (plain as Format) : null;
}

/** How a format is named to the user: `PNG`, `JPEG`, `WebP`, ... */
export function formatLabel(format: string): string {
  const known = formatOf(format);
  return FORMATS.find((entry) => entry.id === known)?.label ?? format.toUpperCase();
}
