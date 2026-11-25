import { useEffect, useRef } from "react";

import { type Registry, describeKey } from "../commands";

/** Every action with a key, by group. */
export function KeyList({ registry, mac, onClose }: { registry: Registry; mac: boolean; onClose: () => void }) {
  const dialog = useRef<HTMLDivElement>(null);
  useEffect(() => dialog.current?.focus(), []);
  const groups = new Map<string, ReturnType<Registry["all"]>>();
  for (const action of registry.all().filter((a) => a.keys.length > 0)) {
    groups.set(action.group, [...(groups.get(action.group) ?? []), action]);
  }
  return (
    <div className="overlay" onMouseDown={onClose}>
      <div
        ref={dialog}
        tabIndex={-1}
        className="key-list"
        role="dialog"
        aria-label="Keyboard shortcuts"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
      >
        <h2>Keyboard shortcuts</h2>
        {[...groups].map(([group, actions]) => (
          <section key={group}>
            <h3>{group}</h3>
            <dl>
              {actions.map((action) => (
                <div key={action.id} className="key-row">
                  <dt>{action.name}</dt>
                  <dd>
                    {action.keys.map((key) => (
                      <kbd key={key}>{describeKey(key, mac)}</kbd>
                    ))}
                  </dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </div>
  );
}
