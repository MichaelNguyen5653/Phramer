// Colour conversions for the live picker: HSV for the square and hue strip,
// hex for typing.

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export interface Hsv {
  /** 0 to 360. */
  h: number;
  /** 0 to 1. */
  s: number;
  /** 0 to 1. */
  v: number;
}

const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

export function hsvToRgb({ h, s, v }: Hsv): Rgb {
  const hue = (((h % 360) + 360) % 360) / 60;
  const sat = clamp(s, 0, 1);
  const val = clamp(v, 0, 1);
  const chroma = val * sat;
  const x = chroma * (1 - Math.abs((hue % 2) - 1));
  const [r, g, b] =
    hue < 1 ? [chroma, x, 0] :
    hue < 2 ? [x, chroma, 0] :
    hue < 3 ? [0, chroma, x] :
    hue < 4 ? [0, x, chroma] :
    hue < 5 ? [x, 0, chroma] : [chroma, 0, x];
  const m = val - chroma;
  const byte = (c: number) => clamp(Math.round((c + m) * 255), 0, 255);
  return { r: byte(r), g: byte(g), b: byte(b) };
}

/** HSV of a colour. Greys keep `hueHint`, so dragging to grey and back does
 * not lose the hue. */
export function rgbToHsv({ r, g, b }: Rgb, hueHint = 0): Hsv {
  const [rf, gf, bf] = [r / 255, g / 255, b / 255];
  const max = Math.max(rf, gf, bf);
  const min = Math.min(rf, gf, bf);
  const delta = max - min;
  let h = hueHint;
  if (delta > 0) {
    if (max === rf) h = 60 * (((gf - bf) / delta) % 6);
    else if (max === gf) h = 60 * ((bf - rf) / delta + 2);
    else h = 60 * ((rf - gf) / delta + 4);
    if (h < 0) h += 360;
  }
  return { h, s: max === 0 ? 0 : delta / max, v: max };
}

export function toHex({ r, g, b }: Rgb): string {
  const part = (v: number) => clamp(Math.round(v), 0, 255).toString(16).padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`;
}

/** `#rgb`, `#rrggbb`, with or without `#`; `null` when it is not a colour. */
export function parseHex(text: string): Rgb | null {
  const hex = text.trim().replace(/^#/, "");
  if (/^[0-9a-f]{3}$/i.test(hex)) {
    const [r, g, b] = [...hex].map((c) => parseInt(c + c, 16));
    return { r, g, b };
  }
  if (/^[0-9a-f]{6}$/i.test(hex)) {
    return {
      r: parseInt(hex.slice(0, 2), 16),
      g: parseInt(hex.slice(2, 4), 16),
      b: parseInt(hex.slice(4, 6), 16),
    };
  }
  return null;
}

export function sameRgb(a: Rgb, b: Rgb): boolean {
  return a.r === b.r && a.g === b.g && a.b === b.b;
}
