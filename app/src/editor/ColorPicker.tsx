// The live colour picker: the palette (`userColors`), a saturation/value
// square, a hue strip, a hex field and the eyedropper. Every pick applies at
// once, while the pointer moves; there is no OK. Closing it keeps the colour
// last picked, and undo takes a pick back.

import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { hsvToRgb, parseHex, rgbToHsv, sameRgb, toHex, type Rgb } from "./color";
import { Icon } from "./Icon";

export interface ColorPickerProps {
  color: Rgb;
  hue: number;
  palette: (Rgb | null)[];
  eyedropper: boolean;
  /** Where the picker sits, fixed in the window. */
  position: { left: number; top: number };
  /** A colour to show and draw with now. */
  onPick(color: Rgb, hue?: number): void;
  /** A gesture finished with `color`: one undo step. */
  onCommit(color: Rgb): void;
  onEyedropper(on: boolean): void;
}

function fraction(event: ReactPointerEvent<HTMLElement>): { x: number; y: number } {
  const box = event.currentTarget.getBoundingClientRect();
  const clamp = (v: number) => Math.min(1, Math.max(0, v));
  return {
    x: clamp((event.clientX - box.left) / Math.max(1, box.width)),
    y: clamp((event.clientY - box.top) / Math.max(1, box.height)),
  };
}

export function ColorPicker(props: ColorPickerProps) {
  const { color, hue, palette, eyedropper, position, onPick, onCommit, onEyedropper } = props;
  const hsv = rgbToHsv(color, hue);
  const [hex, setHex] = useState(toHex(color));
  const editingHex = useRef(false);
  const last = useRef(color);
  last.current = color;

  useEffect(() => {
    if (!editingHex.current) setHex(toHex(color));
  }, [color]);

  // A drag on the square or the strip: applied on every move, kept on release
  const drag = (pick: (event: ReactPointerEvent<HTMLElement>) => void) => ({
    onPointerDown(event: ReactPointerEvent<HTMLElement>) {
      if (event.button !== 0) return;
      event.currentTarget.setPointerCapture(event.pointerId);
      pick(event);
    },
    onPointerMove(event: ReactPointerEvent<HTMLElement>) {
      if (event.currentTarget.hasPointerCapture(event.pointerId)) pick(event);
    },
    onPointerUp(event: ReactPointerEvent<HTMLElement>) {
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
        onCommit(last.current);
      }
    },
  });

  const square = drag((event) => {
    const at = fraction(event);
    const picked = hsvToRgb({ h: hue, s: at.x, v: 1 - at.y });
    last.current = picked;
    onPick(picked, hue);
  });
  const strip = drag((event) => {
    const h = fraction(event).x * 360;
    const picked = hsvToRgb({ h, s: hsv.s, v: hsv.v });
    last.current = picked;
    onPick(picked, h);
  });

  const pure = hsvToRgb({ h: hue, s: 1, v: 1 });
  return (
    <div
      className="picker"
      role="dialog"
      aria-label="Colour"
      data-testid="color-picker"
      style={{ left: position.left, top: position.top }}
      onPointerDown={(event) => event.stopPropagation()}
    >
      <div className="picker-palette" role="listbox" aria-label="Palette">
        {palette.map((entry, index) =>
          entry ? (
            <button
              key={index}
              type="button"
              role="option"
              aria-selected={sameRgb(entry, color)}
              aria-label={toHex(entry)}
              title={toHex(entry)}
              className="swatch"
              style={{ backgroundColor: toHex(entry) }}
              onPointerDown={(event) => {
                if (event.button !== 0) return;
                onPick(entry);
                onCommit(entry);
              }}
            />
          ) : null,
        )}
      </div>
      <div
        className="picker-square"
        data-testid="picker-square"
        style={{ backgroundColor: toHex(pure) }}
        {...square}
      >
        <span
          className="picker-thumb"
          style={{ transform: `translate(calc(${hsv.s} * var(--picker-size)), calc(${1 - hsv.v} * var(--picker-size) * 0.75))` }}
        />
      </div>
      <div className="picker-hue" data-testid="picker-hue" {...strip}>
        <span className="picker-hue-thumb" style={{ transform: `translateX(calc(${hue / 360} * var(--picker-size)))` }} />
      </div>
      <div className="picker-row">
        <span className="swatch current" data-testid="picker-current" style={{ backgroundColor: toHex(color) }} />
        <input
          className="hex"
          aria-label="Hex colour"
          data-testid="picker-hex"
          spellCheck={false}
          value={hex}
          onFocus={() => {
            editingHex.current = true;
          }}
          onChange={(event) => {
            setHex(event.target.value);
            const parsed = parseHex(event.target.value);
            if (parsed) {
              last.current = parsed;
              onPick(parsed);
            }
          }}
          onBlur={() => {
            editingHex.current = false;
            setHex(toHex(last.current));
            onCommit(last.current);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
          }}
        />
        <button
          type="button"
          className={`icon-button${eyedropper ? " active" : ""}`}
          aria-pressed={eyedropper}
          aria-label="Pick a colour from the image"
          title="Pick a colour from the image"
          data-testid="eyedropper"
          onClick={() => onEyedropper(!eyedropper)}
        >
          <Icon name="pipette" />
        </button>
      </div>
    </div>
  );
}
