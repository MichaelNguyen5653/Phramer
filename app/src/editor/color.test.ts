import { describe, expect, it } from "vitest";
import { hsvToRgb, parseHex, rgbToHsv, sameRgb, toHex } from "./color";

describe("colour conversions", () => {
  it("runs the hue circle like the overlay's disc", () => {
    expect(hsvToRgb({ h: 0, s: 1, v: 1 })).toEqual({ r: 255, g: 0, b: 0 });
    expect(hsvToRgb({ h: 120, s: 1, v: 1 })).toEqual({ r: 0, g: 255, b: 0 });
    expect(hsvToRgb({ h: 240, s: 1, v: 1 })).toEqual({ r: 0, g: 0, b: 255 });
    expect(hsvToRgb({ h: -120, s: 1, v: 1 })).toEqual({ r: 0, g: 0, b: 255 });
    expect(hsvToRgb({ h: 0, s: 0, v: 1 })).toEqual({ r: 255, g: 255, b: 255 });
    expect(hsvToRgb({ h: 200, s: 0.5, v: 0 })).toEqual({ r: 0, g: 0, b: 0 });
  });

  it("round-trips every byte through HSV", () => {
    for (const color of [
      { r: 12, g: 200, b: 99 },
      { r: 255, g: 128, b: 0 },
      { r: 1, g: 2, b: 3 },
    ]) {
      expect(hsvToRgb(rgbToHsv(color))).toEqual(color);
    }
  });

  it("keeps the hue of a grey from the hint", () => {
    expect(rgbToHsv({ r: 80, g: 80, b: 80 }, 210).h).toBe(210);
  });

  it("reads and writes hex", () => {
    expect(parseHex("#ff8000")).toEqual({ r: 255, g: 128, b: 0 });
    expect(parseHex("0f0")).toEqual({ r: 0, g: 255, b: 0 });
    expect(parseHex("#12345")).toBeNull();
    expect(parseHex("zzzzzz")).toBeNull();
    expect(toHex({ r: 255, g: 128, b: 0 })).toBe("#ff8000");
    expect(sameRgb({ r: 1, g: 2, b: 3 }, { r: 1, g: 2, b: 3 })).toBe(true);
  });
});
