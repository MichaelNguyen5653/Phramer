// The view: how image pixels map to the viewport. `scale` is device pixels
// per image pixel, so 1 is 100%: one captured pixel on one screen pixel.
// `x` and `y` are where the image's origin sits in the viewport, in CSS
// pixels.

export interface View {
  scale: number;
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

/** 15.0.0's zoom range and step (`canvasgeometry`). */
export const MIN_SCALE = 0.1;
export const MAX_SCALE = 8;
export const STEP = 1.25;
/** Room kept around the image when it is fitted, in CSS pixels. */
export const FIT_MARGIN = 24;

export function clampScale(scale: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

/** One step in or out; a step across 100% stops on it. */
export function stepScale(scale: number, steps: number): number {
  let next = scale;
  const factor = steps > 0 ? STEP : 1 / STEP;
  for (let i = 0; i < Math.abs(steps); i++) {
    const stepped = next * factor;
    next = (next < 1 && stepped > 1) || (next > 1 && stepped < 1) ? 1 : stepped;
  }
  return clampScale(next);
}

/** CSS pixels one image pixel takes at `scale`. */
export function cssPerPixel(scale: number, dpr: number): number {
  return scale / (dpr > 0 ? dpr : 1);
}

/** The image centred in the viewport at `scale`. */
export function centered(scale: number, image: Size, viewport: Size, dpr: number): View {
  const unit = cssPerPixel(scale, dpr);
  return {
    scale,
    x: Math.round((viewport.width - image.width * unit) / 2),
    y: Math.round((viewport.height - image.height * unit) / 2),
  };
}

/** The scale at which the whole image fits the viewport. With `upscale`
 * false (on open) it never goes past 100%. */
export function fitScale(image: Size, viewport: Size, dpr: number, upscale = true): number {
  const room = (length: number) => Math.max(1, length - 2 * FIT_MARGIN);
  const fit = Math.min(
    (room(viewport.width) * dpr) / Math.max(1, image.width),
    (room(viewport.height) * dpr) / Math.max(1, image.height),
  );
  return clampScale(upscale ? fit : Math.min(1, fit));
}

export function fit(image: Size, viewport: Size, dpr: number, upscale = true): View {
  return centered(fitScale(image, viewport, dpr, upscale), image, viewport, dpr);
}

/** Zooms to `scale` keeping the image point under `anchor` (viewport CSS
 * pixels) where it is. */
export function zoomAt(view: View, scale: number, anchor: { x: number; y: number }, dpr: number): View {
  const next = clampScale(scale);
  const before = cssPerPixel(view.scale, dpr);
  const after = cssPerPixel(next, dpr);
  const imageX = (anchor.x - view.x) / before;
  const imageY = (anchor.y - view.y) / before;
  return { scale: next, x: anchor.x - imageX * after, y: anchor.y - imageY * after };
}

export function pan(view: View, dx: number, dy: number): View {
  return { ...view, x: view.x + dx, y: view.y + dy };
}

/** The image pixel under a viewport point (fractional). */
export function toImage(view: View, at: { x: number; y: number }, dpr: number): { x: number; y: number } {
  const unit = cssPerPixel(view.scale, dpr);
  return { x: (at.x - view.x) / unit, y: (at.y - view.y) / unit };
}

/** Where an image point is in the viewport. */
export function toViewport(view: View, at: { x: number; y: number }, dpr: number): { x: number; y: number } {
  const unit = cssPerPixel(view.scale, dpr);
  return { x: view.x + at.x * unit, y: view.y + at.y * unit };
}

/** A wheel turn as zoom steps: one per notch (120), touchpads in fractions
 * rounded to at least one step. */
export function wheelSteps(deltaY: number): number {
  if (deltaY === 0) return 0;
  const notches = -deltaY / 120;
  return notches > 0 ? Math.max(1, Math.round(notches)) : Math.min(-1, Math.round(notches));
}

/** 15.0.0's grid steps, in image pixels. */
const GRID_STEPS = [10, 20, 50, 100, 200, 500, 1000];
/** The closest grid lines may come, in CSS pixels. */
const GRID_MIN_SPACING = 12;

/** The grid's step in image pixels at `unit` CSS pixels per image pixel:
 * the lines stay on the image while zooming, and the step widens as the
 * zoom drops so the grid never turns into a wash (`paintGrid`). Every
 * fifth line is a major one. */
export function gridStep(unit: number): number {
  return GRID_STEPS.find((step) => step * unit >= GRID_MIN_SPACING) ?? GRID_STEPS[GRID_STEPS.length - 1];
}
