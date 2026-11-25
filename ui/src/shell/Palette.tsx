import { useEffect, useRef, useState } from "react";

import { type Registry, describeKey } from "../commands";

/** The command palette: type to find an action, arrows to choose, Enter to run. */
export function Palette({ registry, mac, onClose }: { registry: Registry; mac: boolean; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [chosen, setChosen] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  // Searched afresh each render, so it never shows an action since removed.
  const found = registry.search(query);
  const shown = Math.min(chosen, Math.max(found.length - 1, 0));

  useEffect(() => input.current?.focus(), []);

  const run = (index: number) => {
    const action = found[index];
    if (!action) return;
    onClose();
    action.run();
  };

  return (
    <div className="overlay" onMouseDown={onClose}>
      <div
        className="palette"
        role="dialog"
        aria-label="Command palette"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <input
          ref={input}
          className="palette-input"
          placeholder="Type a command"
          aria-label="Command"
          aria-controls="palette-list"
          aria-activedescendant={found[shown] ? `palette-${found[shown].id}` : undefined}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setChosen(0);
          }}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") setChosen((shown + 1) % Math.max(found.length, 1));
            else if (event.key === "ArrowUp") setChosen((shown - 1 + found.length) % Math.max(found.length, 1));
            else if (event.key === "Enter") run(shown);
            else if (event.key === "Escape") onClose();
            else return;
            event.preventDefault();
            event.stopPropagation();
          }}
        />
        <ul id="palette-list" className="palette-list" role="listbox">
          {found.map((action, index) => (
            <li
              key={action.id}
              id={`palette-${action.id}`}
              role="option"
              aria-selected={index === shown}
              className={index === shown ? "chosen" : undefined}
              onMouseMove={() => setChosen(index)}
              onClick={() => run(index)}
            >
              <span>{action.name}</span>
              <span className="keys">
                {action.keys.map((key) => (
                  <kbd key={key}>{describeKey(key, mac)}</kbd>
                ))}
              </span>
            </li>
          ))}
          {found.length === 0 && <li className="muted">No command matches.</li>}
        </ul>
      </div>
    </div>
  );
}
