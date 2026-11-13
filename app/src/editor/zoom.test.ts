import { describe, expect, it } from "vitest";
import { MAX_SCALE, MIN_SCALE, centered, clampScale, cssPerPixel, fit, fitScale, pan, stepScale, toImage, toViewport, wheelSteps, zoomAt } from "./zoom";

describe("zoom maths", () => {
  it("steps by 1.25 and stops on 100% when crossing it", () => {
    expect(stepScale(1, 1)).toBeCloseTo(1.25);
    expect(stepScale(1, -1)).toBeCloseTo(0.8);
    expect(stepScale(0.9, 1)).toBe(1);
    expect(stepScale(1.1, -1)).toBe(1);
    expect(stepScale(1, 3)).toBeCloseTo(1.953125);
  });

  it("keeps the scale within 10% and 800%", () => {
    expect(clampScale(0.01)).toBe(MIN_SCALE);
    expect(clampScale(100)).toBe(MAX_SCALE);
    expect(stepScale(MAX_SCALE, 1)).toBe(MAX_SCALE);
    expect(stepScale(MIN_SCALE, -1)).toBe(MIN_SCALE);
  });

  it("counts 100% in device pixels", () => {
    expect(cssPerPixel(1, 2)).toBe(0.5);
    expect(cssPerPixel(2, 1)).toBe(2);
  });

  it("fits and centres, never past 100% when opening", () => {
    const image = { width: 2000, height: 1000 };
    const viewport = { width: 1048, height: 1048 };
    expect(fitScale(image, viewport, 1)).toBeCloseTo(0.5);
    expect(fitScale({ width: 100, height: 100 }, viewport, 1, false)).toBe(1);
    expect(fitScale({ width: 100, height: 100 }, viewport, 1, true)).toBe(MAX_SCALE);
    const view = fit(image, viewport, 1);
    expect(view).toEqual({ scale: 0.5, x: 24, y: 274 });
    expect(centered(1, { width: 100, height: 50 }, { width: 300, height: 300 }, 2)).toEqual({ scale: 1, x: 125, y: 138 });
  });

  it("keeps the point under the pointer still while zooming", () => {
    const view = { scale: 1, x: 40, y: 30 };
    const anchor = { x: 140, y: 230 };
    const before = toImage(view, anchor, 1.5);
    const zoomed = zoomAt(view, 2.5, anchor, 1.5);
    const after = toImage(zoomed, anchor, 1.5);
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
  });

  it("maps image and viewport points both ways", () => {
    const view = { scale: 2, x: 10, y: -5 };
    const at = toViewport(view, { x: 7, y: 9 }, 1);
    expect(at).toEqual({ x: 24, y: 13 });
    expect(toImage(view, at, 1)).toEqual({ x: 7, y: 9 });
    expect(pan(view, 3, 4)).toEqual({ scale: 2, x: 13, y: -1 });
  });

  it("turns wheel deltas into whole steps", () => {
    expect(wheelSteps(-120)).toBe(1);
    expect(wheelSteps(240)).toBe(-2);
    expect(wheelSteps(-4)).toBe(1);
    expect(wheelSteps(0)).toBe(0);
  });
});
