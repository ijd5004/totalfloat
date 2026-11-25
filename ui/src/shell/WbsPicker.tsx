// Asks which WBS node the selected activities move to: every node, in the
// WBS's order and indented by depth, filtered by what's typed. Enter or a
// click picks the one marked; Escape closes.

import { useEffect, useMemo, useRef, useState } from "react";

/** A node to pick: its dotted path, name and depth. */
export interface WbsChoice {
  path: string;
  name: string;
  depth: number;
}

/** Every node in `order`, in order: its siblings' order at each level, as the grid's bands sort. */
export function wbsChoices(order: ReadonlyMap<string, number>, names: ReadonlyMap<string, string> | null): WbsChoice[] {
  const key = (path: string) => {
    const parts = path.split(".");
    return parts.map((_, k) => order.get(parts.slice(0, k + 1).join(".")) ?? Number.POSITIVE_INFINITY);
  };
  const keyed = [...order.keys()].map((path) => ({ path, key: key(path) }));
  keyed.sort((a, b) => {
    for (let k = 0; k < Math.min(a.key.length, b.key.length); k++) {
      const by = (a.key[k] ?? 0) - (b.key[k] ?? 0);
      if (by !== 0) return by;
    }
    return a.key.length - b.key.length || a.path.localeCompare(b.path);
  });
  return keyed.map(({ path }) => ({ path, name: names?.get(path) ?? "", depth: path.split(".").length - 1 }));
}

export function WbsPicker({
  title,
  choices,
  onPick,
  onClose,
}: {
  title: string;
  choices: readonly WbsChoice[];
  onPick: (path: string) => void;
  onClose: () => void;
}) {
  const [text, setText] = useState("");
  const [at, setAt] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const shown = useMemo(() => {
    const wanted = text.trim().toLowerCase();
    return wanted === "" ? choices : choices.filter((c) => c.path.toLowerCase().includes(wanted) || c.name.toLowerCase().includes(wanted));
  }, [choices, text]);
  const marked = Math.min(at, shown.length - 1);
  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="wbs-picker" role="dialog" aria-modal="true" aria-labelledby="wbs-picker-title" onMouseDown={(event) => event.stopPropagation()}>
        <h2 id="wbs-picker-title">{title}</h2>
        <input
          ref={input}
          type="text"
          role="combobox"
          aria-expanded="true"
          aria-controls="wbs-picker-list"
          aria-activedescendant={marked >= 0 ? `wbs-choice-${marked}` : undefined}
          aria-label="Find a WBS node by code or name"
          placeholder="Find a node by code or name"
          value={text}
          onChange={(event) => {
            setText(event.target.value);
            setAt(0);
          }}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              const by = event.key === "ArrowDown" ? 1 : -1;
              setAt(Math.max(0, Math.min(shown.length - 1, marked + by)));
            } else if (event.key === "Enter") {
              event.preventDefault();
              const choice = shown[marked];
              if (choice) onPick(choice.path);
            } else if (event.key === "Escape") {
              event.preventDefault();
              onClose();
            }
          }}
        />
        <ul id="wbs-picker-list" role="listbox" aria-label="WBS nodes">
          {shown.map((choice, k) => (
            <li
              key={choice.path}
              id={`wbs-choice-${k}`}
              role="option"
              aria-selected={k === marked}
              style={{ paddingLeft: `${0.5 + choice.depth}rem` }}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => onPick(choice.path)}
            >
              {choice.name ? <strong>{choice.name}</strong> : null}
              <span className="muted">{choice.path}</span>
            </li>
          ))}
          {shown.length === 0 && <li className="muted">No node matches.</li>}
        </ul>
      </div>
    </div>
  );
}
