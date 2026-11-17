// The toolbar buttons (`buttons`): 15.0.0's Button Selection list as a grid
// of toggles, with its Select All.

import { Icon } from "./Icon";
import type { ToolbarChoice } from "./types";

export function ToolbarEditor(props: { value: number[]; choices: ToolbarChoice[]; onChange(ids: number[]): void }) {
  const on = new Set(props.value);
  const toggle = (id: number) => {
    const next = props.choices.map((choice) => choice.id).filter((choice) => (choice === id ? !on.has(id) : on.has(choice)));
    props.onChange(next);
  };
  return (
    <div className="toolbar-editor">
      <div className="toolbar-grid" role="group" aria-label="Toolbar buttons">
        {props.choices.map((choice) => (
          <button
            key={choice.id}
            type="button"
            className="toolbar-choice"
            aria-pressed={on.has(choice.id)}
            title={choice.description}
            data-testid={`toolbar-${choice.id}`}
            onClick={() => toggle(choice.id)}
          >
            <Icon name={choice.icon} />
            <span>{choice.name}</span>
          </button>
        ))}
      </div>
      <button
        type="button"
        className="text-button"
        disabled={props.choices.every((choice) => on.has(choice.id))}
        onClick={() => props.onChange(props.choices.map((choice) => choice.id))}
      >
        Select all
      </button>
    </div>
  );
}
