// The side panel: what the selected tool (or the selected object) draws
// with. Every control applies as it changes; nothing waits for an OK.

import { toHex, type Rgb } from "./color";
import type { Annotation, ArrowHead, RedactStyle, ShapeStyle, TextAlign } from "./engine";
import { Icon } from "./Icon";
import { MAX_TOOL_SIZE, TOOLS, type ShapeKind, type ToolId, type ToolOptions } from "./tools";

export interface OptionsPanelProps {
  tool: ToolId | null;
  color: Rgb;
  size: number;
  options: ToolOptions;
  selected: Annotation | null;
  nextCounter: number;
  pickerOpen: boolean;
  onColorButton(button: HTMLElement): void;
  /** While the slider moves. */
  onSizePreview(size: number): void;
  /** The slider let go, a number typed. */
  onSize(size: number): void;
  onOptions(options: Partial<ToolOptions>): void;
  onNextCounter(next: number): void;
  onDelete(): void;
}

function Segmented<T extends string>(props: {
  label: string;
  value: T;
  choices: [T, string][];
  onChange(value: T): void;
}) {
  return (
    <div className="field">
      <span className="field-label">{props.label}</span>
      <div className="segmented" role="radiogroup" aria-label={props.label}>
        {props.choices.map(([value, text]) => (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={props.value === value}
            className={props.value === value ? "active" : undefined}
            onClick={() => props.onChange(value)}
          >
            {text}
          </button>
        ))}
      </div>
    </div>
  );
}

function Toggle(props: { label: string; checked: boolean; onChange(checked: boolean): void }) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={props.checked} onChange={(event) => props.onChange(event.target.checked)} />
      <span>{props.label}</span>
    </label>
  );
}

const STYLES: [ShapeStyle, string][] = [
  ["outline", "Hollow"],
  ["filled", "Filled"],
  ["highlight", "Highlight"],
];

export function OptionsPanel(props: OptionsPanelProps) {
  const { tool, options, onOptions } = props;
  const toolName = tool ? TOOLS.find((entry) => entry.id === tool)?.name : null;
  const kind = props.selected?.shape.tool;
  const title = toolName ?? (props.selected ? "Selected object" : "No tool");
  const shows = (...tools: ToolId[]) => tool !== null && tools.includes(tool);
  const highlight =
    (shows("shape") && options.shapeStyle === "highlight") ||
    (shows("rectangle") && options.rectangleStyle === "highlight");

  return (
    <aside className="panel" aria-label="Tool options" data-testid="options-panel">
      <h2 className="panel-title">{title}</h2>

      <div className="field">
        <span className="field-label">Colour</span>
        <button
          type="button"
          className={`color-button${props.pickerOpen ? " active" : ""}`}
          data-testid="color-button"
          aria-haspopup="dialog"
          aria-expanded={props.pickerOpen}
          onClick={(event) => props.onColorButton(event.currentTarget)}
        >
          <span className="swatch" style={{ backgroundColor: toHex(props.color) }} />
          <span className="mono">{toHex(props.color)}</span>
        </button>
      </div>

      <div className="field">
        <span className="field-label">{shows("text") ? "Font size" : "Size"}</span>
        <div className="size-row">
          <input
            type="range"
            min={1}
            max={MAX_TOOL_SIZE}
            value={props.size}
            aria-label="Size"
            data-testid="size-slider"
            onChange={(event) => props.onSizePreview(Number(event.target.value))}
            onPointerUp={(event) => props.onSize(Number(event.currentTarget.value))}
            onKeyUp={(event) => props.onSize(Number(event.currentTarget.value))}
          />
          <input
            type="number"
            min={1}
            max={MAX_TOOL_SIZE}
            className="number"
            value={props.size}
            aria-label="Size value"
            onChange={(event) => props.onSize(Number(event.target.value))}
          />
        </div>
      </div>

      {shows("shape") && (
        <>
          <Segmented<ShapeKind>
            label="Shape"
            value={options.shapeKind}
            choices={[
              ["square", "Square"],
              ["circle", "Circle"],
            ]}
            onChange={(shapeKind) => onOptions({ shapeKind })}
          />
          <Segmented<ShapeStyle> label="Style" value={options.shapeStyle} choices={STYLES} onChange={(shapeStyle) => onOptions({ shapeStyle })} />
        </>
      )}
      {shows("rectangle") && (
        <Segmented<ShapeStyle>
          label="Fill"
          value={options.rectangleStyle}
          choices={[
            ["filled", "Filled"],
            ["highlight", "Highlight"],
          ]}
          onChange={(rectangleStyle) => onOptions({ rectangleStyle })}
        />
      )}
      {highlight && (
        <div className="field">
          <span className="field-label">Highlight border</span>
          <input
            type="number"
            min={0}
            max={20}
            className="number"
            value={options.highlightOutline}
            aria-label="Highlight border"
            onChange={(event) => onOptions({ highlightOutline: Math.min(20, Math.max(0, Number(event.target.value) || 0)) })}
          />
        </div>
      )}
      {shows("arrow") && (
        <>
          <Segmented<ArrowHead>
            label="Head"
            value={options.arrowHead}
            choices={[
              ["default", "Triangle"],
              ["curved", "Curved"],
            ]}
            onChange={(arrowHead) => onOptions({ arrowHead })}
          />
          <Toggle label="Head at the start" checked={options.reverseArrow} onChange={(reverseArrow) => onOptions({ reverseArrow })} />
        </>
      )}
      {shows("text") && (
        <>
          <Segmented<TextAlign>
            label="Align"
            value={options.textAlign}
            choices={[
              ["left", "Left"],
              ["center", "Centre"],
              ["right", "Right"],
            ]}
            onChange={(textAlign) => onOptions({ textAlign })}
          />
          <Toggle label="Underline" checked={options.underline} onChange={(underline) => onOptions({ underline })} />
          <Toggle label="Strikeout" checked={options.strikeout} onChange={(strikeout) => onOptions({ strikeout })} />
        </>
      )}
      {shows("circle_count") && (
        <div className="field">
          <span className="field-label">Next number</span>
          <input
            type="number"
            min={1}
            max={999}
            className="number"
            value={props.nextCounter}
            aria-label="Next number"
            onChange={(event) => props.onNextCounter(Number(event.target.value) || 1)}
          />
        </div>
      )}
      {shows("pixelate") && (
        <Segmented<RedactStyle>
          label="Method"
          value={options.redact}
          choices={[
            ["frosted", "Secure"],
            ["insecure", "Blur"],
          ]}
          onChange={(redact) => onOptions({ redact })}
        />
      )}

      {props.selected && (
        <div className="field">
          <span className="field-label">Selected {kind?.replace("_", " ")}</span>
          <button type="button" className="text-button" onClick={props.onDelete}>
            <Icon name="x" /> Delete
          </button>
        </div>
      )}
    </aside>
  );
}
