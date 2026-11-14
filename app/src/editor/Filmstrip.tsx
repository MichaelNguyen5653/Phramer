// The strip under the image when the window holds more than one: a
// thumbnail per image (15.0.0's filmstrip, numbered from 1, marked when not
// saved), and what acts on the set: Save All and Remove Image.

import type { Doc, Page } from "./images";
import { Icon } from "./Icon";

export interface FilmstripProps<D extends Doc> {
  pages: readonly Page<D>[];
  current: number;
  savingAll: boolean;
  onShow(index: number): void;
  onSaveAll(): void;
  onRemove(): void;
}

export function Filmstrip<D extends Doc>(props: FilmstripProps<D>) {
  return (
    <section className="filmstrip" aria-label="Images" data-testid="filmstrip">
      <div className="filmstrip-actions">
        <button
          type="button"
          className="text-button"
          title="Save every image in this session to one folder"
          disabled={props.savingAll}
          data-testid="save-all"
          onClick={props.onSaveAll}
        >
          <Icon name="folder-down" />
          Save All
        </button>
        <button
          type="button"
          className="text-button"
          title="Remove the current image from this editor session"
          data-testid="remove-image"
          onClick={props.onRemove}
        >
          <Icon name="trash-2" />
          Remove Image
        </button>
      </div>
      <ol className="thumbs">
        {props.pages.map((page, index) => {
          const unsaved = page.doc.dirty;
          return (
            <li key={page.id}>
              <button
                type="button"
                className={`thumb${index === props.current ? " active" : ""}`}
                aria-current={index === props.current}
                aria-label={`Image ${index + 1}${unsaved ? ", not saved" : ""}`}
                title={`Image ${index + 1}${unsaved ? " (not saved)" : ""}`}
                data-testid={`thumb-${index + 1}`}
                onClick={() => props.onShow(index)}
              >
                {page.thumbnail && <img src={page.thumbnail} alt="" draggable={false} />}
                <span className="thumb-number">{index + 1}</span>
                {unsaved && <span className="thumb-unsaved" data-testid="unsaved" aria-hidden="true" />}
              </button>
            </li>
          );
        })}
      </ol>
    </section>
  );
}
