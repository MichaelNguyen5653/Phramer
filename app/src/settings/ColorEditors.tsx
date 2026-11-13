// Colours: a single colour (uiColor, contrastUiColor) and the palette
// (userColors), both edited with the editor's live colour picker. The
// picker shows each pick at once; a finished gesture is saved.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ColorPicker } from "../editor/ColorPicker";
import { parseHex, rgbToHsv, toHex, type Rgb } from "../editor/color";
import { Icon } from "./Icon";
import { PALETTE_MAX, PALETTE_MIN } from "./validate";

/** The picker's size, for keeping it inside the window. */
const PICKER_WIDTH = 248;
const PICKER_HEIGHT = 330;
const GAP = 6;

interface EyeDropperResult {
  sRGBHex: string;
}
declare global {
  interface Window {
    EyeDropper?: new () => { open(): Promise<EyeDropperResult> };
  }
}

/** Below the anchor, or above it when there is no room; inside the window. */
function placeBy(anchor: DOMRect): { left: number; top: number } {
  const left = Math.min(Math.max(GAP, anchor.left), window.innerWidth - PICKER_WIDTH - GAP);
  const below = anchor.bottom + GAP;
  const top = below + PICKER_HEIGHT <= window.innerHeight ? below : Math.max(GAP, anchor.top - PICKER_HEIGHT - GAP);
  return { left, top };
}

function rgb(hex: string | null): Rgb {
  return (hex && parseHex(hex)) || { r: 255, g: 255, b: 255 };
}

/** The picker as a popover: closes on Esc or a press outside it. */
function Popover(props: {
  anchor: HTMLElement;
  color: Rgb;
  palette: (Rgb | null)[];
  onPick(color: Rgb): void;
  onCommit(color: Rgb): void;
  onClose(): void;
}) {
  const [hue, setHue] = useState(() => rgbToHsv(props.color).h);
  const [position, setPosition] = useState(() => placeBy(props.anchor.getBoundingClientRect()));
  useLayoutEffect(() => setPosition(placeBy(props.anchor.getBoundingClientRect())), [props.anchor]);

  useEffect(() => {
    const away = (event: PointerEvent) => {
      if (!props.anchor.contains(event.target as Node)) props.onClose();
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        props.onClose();
      }
    };
    window.addEventListener("pointerdown", away);
    window.addEventListener("keydown", key, true);
    return () => {
      window.removeEventListener("pointerdown", away);
      window.removeEventListener("keydown", key, true);
    };
  }, [props]);

  return (
    <ColorPicker
      color={props.color}
      hue={hue}
      palette={props.palette}
      eyedropper={false}
      position={position}
      onPick={(color, nextHue) => {
        if (nextHue !== undefined) setHue(nextHue);
        else setHue(rgbToHsv(color, hue).h);
        props.onPick(color);
      }}
      onCommit={props.onCommit}
      onEyedropper={() => {
        // The screen's colours, through the browser's own eyedropper
        const Dropper = window.EyeDropper;
        if (!Dropper) return;
        new Dropper().open().then(
          (result) => {
            const picked = parseHex(result.sRGBHex);
            if (picked) {
              setHue(rgbToHsv(picked, hue).h);
              props.onPick(picked);
              props.onCommit(picked);
            }
          },
          () => {},
        );
      }}
    />
  );
}

export function ColorField(props: {
  value: string;
  label: string;
  palette: (string | null)[];
  onChange(hex: string): void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLDivElement>(null);
  const shown = draft ?? props.value;
  return (
    <div className="color-field" ref={anchor}>
      <button
        type="button"
        className={`color-button${open ? " active" : ""}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`${props.label}: ${shown}`}
        data-testid={`color-${props.label}`}
        onClick={() => setOpen(!open)}
      >
        <span className="swatch" style={{ backgroundColor: shown }} />
        <span className="mono">{shown}</span>
      </button>
      {open && anchor.current && (
        <Popover
          anchor={anchor.current}
          color={rgb(shown)}
          palette={props.palette.map((entry) => (entry ? rgb(entry) : null))}
          onPick={(color) => setDraft(toHex(color))}
          onCommit={(color) => {
            setDraft(null);
            props.onChange(toHex(color));
          }}
          onClose={() => setOpen(false)}
        />
      )}
    </div>
  );
}

/** A colour not yet in the palette, for the + button. */
const CANDIDATES = ["#ffffff", "#000000", "#ff0000", "#00ff00", "#0000ff", "#ffff00", "#00ffff", "#ff00ff", "#808080"];

export function PaletteEditor(props: {
  value: (string | null)[];
  onChange(palette: (string | null)[]): void;
  onProblem(problem: string | null): void;
}) {
  const [selected, setSelected] = useState<number | null>(null);
  const [draft, setDraft] = useState<(string | null)[] | null>(null);
  const anchor = useRef<HTMLDivElement>(null);
  const palette = draft ?? props.value;
  const colors = palette.filter((entry): entry is string => entry !== null);

  const replace = (index: number, hex: string) => palette.map((entry, at) => (at === index ? hex : entry));

  const add = () => {
    if (palette.length >= PALETTE_MAX) {
      props.onProblem(`The palette holds at most ${PALETTE_MAX} entries`);
      return;
    }
    props.onProblem(null);
    const hex = CANDIDATES.find((candidate) => !colors.includes(candidate)) ?? "#ffffff";
    const next = [...palette, hex];
    props.onChange(next);
    setSelected(next.length - 1);
  };

  const remove = () => {
    if (selected === null) return;
    if (palette.length <= PALETTE_MIN) {
      props.onProblem(`The palette needs at least ${PALETTE_MIN} entries`);
      return;
    }
    props.onProblem(null);
    props.onChange(palette.filter((_, at) => at !== selected));
    setSelected(null);
  };

  return (
    <div className="palette" ref={anchor}>
      <div className="palette-swatches" role="listbox" aria-label="Palette colours">
        {palette.map((entry, index) =>
          entry === null ? (
            <span key={index} className="swatch custom" title="Custom colour (the picker)" aria-label="Custom colour" role="option" aria-selected={false} />
          ) : (
            <button
              key={index}
              type="button"
              role="option"
              aria-selected={selected === index}
              aria-label={entry}
              title={entry}
              className="swatch"
              style={{ backgroundColor: entry }}
              data-testid={`palette-${index}`}
              onClick={() => setSelected(selected === index ? null : index)}
            />
          ),
        )}
        <button type="button" className="icon-button palette-add" aria-label="Add a colour" title="Add a colour" onClick={add}>
          <Icon name="plus" />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label="Remove the selected colour"
          title="Remove the selected colour"
          disabled={selected === null}
          onClick={remove}
        >
          <Icon name="trash-2" />
        </button>
      </div>
      {selected !== null && palette[selected] && anchor.current && (
        <Popover
          key={selected}
          anchor={anchor.current}
          color={rgb(palette[selected])}
          palette={[]}
          onPick={(color) => setDraft(replace(selected, toHex(color)))}
          onCommit={(color) => {
            const next = replace(selected, toHex(color));
            setDraft(null);
            props.onChange(next);
          }}
          onClose={() => setSelected(null)}
        />
      )}
    </div>
  );
}
