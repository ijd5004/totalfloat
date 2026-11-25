// The Menu button: one menu with File, Edit, View, Schedule and Help
// sections, built from the command registry, so the menu, the command
// palette and the key list always agree. Keyboard as a WAI-ARIA menu button:
// Enter, Space or Down opens it at its first item that can run, Up at its
// last; arrows, Home and End move; Enter runs; Escape closes. Alt+Shift+M,
// or Alt pressed and let go on its own, opens it from anywhere but a text
// field or a dialog. Edge and Chrome keep F10 for their own toolbars, so the
// app doesn't use it.

import { type KeyboardEvent, useCallback, useEffect, useId, useRef, useState } from "react";

import { type Action, type Registry, describeKey, typingIn } from "../commands";
import { Icon } from "./icons";

export const SECTIONS = ["File", "Edit", "View", "Schedule", "Help"] as const;
export type Section = (typeof SECTIONS)[number];

/** Each section's actions, in the order they were registered. */
export function sectionsOf(registry: Registry): Map<Section, Action[]> {
  const out = new Map<Section, Action[]>(SECTIONS.map((section) => [section, []]));
  for (const action of registry.inOrder()) out.get(action.group as Section)?.push(action);
  return out;
}

/** Whether an action can run now. */
const runnable = (action: Action) => !action.enabled || action.enabled();

export function MenuButton({ registry, mac }: { registry: Registry; mac: boolean }) {
  const [open, setOpen] = useState(false);
  // The item focused, by its place among every item, top to bottom.
  const [item, setItem] = useState(0);
  const button = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const returnTo = useRef<Element | null>(null);
  const id = useId();
  const sections = open ? sectionsOf(registry) : null;
  const items = sections ? SECTIONS.flatMap((section) => sections.get(section) ?? []) : [];

  /** Remembers where focus was before the menu took it, to give it back. */
  const remember = useCallback(() => {
    const active = document.activeElement;
    if (button.current === active || list.current?.contains(active)) return;
    returnTo.current = active === document.body ? null : active;
  }, []);

  const close = useCallback((refocus: "button" | "work" | null) => {
    setOpen(false);
    if (refocus === "button") button.current?.focus();
    if (refocus === "work") {
      // Back to where the work was; the grid if nothing was focused.
      const back = returnTo.current instanceof HTMLElement && returnTo.current.isConnected ? returnTo.current : null;
      (back ?? document.querySelector<HTMLElement>(".grid-body"))?.focus();
    }
    if (refocus !== "button") returnTo.current = null;
  }, []);

  /** Opens the menu at its first item that can run, or with `last`, its last. */
  const openAt = useCallback(
    (last = false) => {
      remember();
      const all = SECTIONS.flatMap((section) => sectionsOf(registry).get(section) ?? []);
      const order = last ? [...all.keys()].reverse() : [...all.keys()];
      setItem(order.find((k) => all[k] && runnable(all[k])) ?? 0);
      setOpen(true);
    },
    [registry, remember],
  );

  // Alt+Shift+M, or Alt pressed and let go on its own, opens the menu,
  // or closes it when open. Its keys are a registered action, so the key
  // list shows them; a lone Alt can't be a binding.
  const isOpen = useRef(open);
  useEffect(() => {
    isOpen.current = open;
  }, [open]);
  useEffect(() => {
    const toggle = () => {
      if (isOpen.current) close("work");
      else openAt();
    };
    let lone = false;
    // Not from a field being typed in, nor from under a dialog: taking focus
    // there would end the typing or leave the dialog stranded.
    const busy = () => typingIn(document.activeElement) || document.querySelector('[role="dialog"]') !== null;
    const down = (event: globalThis.KeyboardEvent) => {
      lone = event.key === "Alt" && !event.repeat;
    };
    const unregister = registry.register({
      id: "menu.open",
      name: "Open the Menu (or press Alt on its own)",
      group: "Menu",
      keys: ["alt+shift+m"],
      inPalette: false,
      enabled: () => isOpen.current || !busy(),
      run: toggle,
    });
    const up = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Alt" && lone && !busy()) {
        event.preventDefault();
        toggle();
      }
      lone = false;
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      unregister();
    };
  }, [registry, openAt, close]);

  // A click anywhere else, or focus going elsewhere (as when Ctrl+K opens
  // the palette), closes the menu.
  useEffect(() => {
    if (!open) return;
    const away = (event: Event) => {
      const target = event.target as Node;
      if (!list.current?.contains(target) && !button.current?.contains(target)) close(null);
    };
    window.addEventListener("mousedown", away);
    window.addEventListener("focusin", away);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("focusin", away);
    };
  }, [open, close]);

  // The chosen item takes focus.
  useEffect(() => {
    if (!open) return;
    list.current?.querySelectorAll<HTMLElement>('[role="menuitem"]')[item]?.focus();
  }, [open, item]);

  const run = (action: Action) => {
    if (!runnable(action)) return;
    close("work");
    action.run();
  };

  const onButtonKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      openAt();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      openAt(true);
    } else if (event.key === "Escape") {
      // Escape on the button goes back to the work: where it was, or the grid.
      event.preventDefault();
      close("work");
    }
  };

  const onItemKey = (event: KeyboardEvent<HTMLButtonElement>, at: number) => {
    const count = items.length;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setItem((((at + (event.key === "ArrowDown" ? 1 : -1)) % count) + count) % count);
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      setItem(event.key === "Home" ? 0 : count - 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close("button");
    } else if (event.key === "Tab") {
      close(null);
    } else if (event.key.length === 1 && /\S/.test(event.key) && !event.ctrlKey && !event.metaKey && !event.altKey) {
      // Type-ahead: the next item whose label starts with the letter.
      const letter = event.key.toLowerCase();
      for (let step = 1; step <= count; step++) {
        const next = (at + step) % count;
        if ((items[next]?.label ?? items[next]?.name ?? "").toLowerCase().startsWith(letter)) {
          setItem(next);
          break;
        }
      }
    }
  };

  let at = -1;
  return (
    <div className="menu-button">
      <button
        ref={button}
        type="button"
        className="title-button"
        title={`Menu (${describeKey("alt+shift+m", mac)})`}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onMouseDown={remember}
        onClick={(event) => {
          // Opened by Space or Enter already, as the key went down.
          if (event.detail === 0 && open) return;
          if (open) close(null);
          else openAt();
        }}
        onKeyDown={onButtonKey}
        onKeyUp={(event) => {
          // Space clicks as it comes up in some browsers; the key opened it already.
          if (event.key === " ") event.preventDefault();
        }}
      >
        <Icon name="menu" />
        <span className="title-words">Menu</span>
      </button>
      {open && sections && (
        <div ref={list} id={id} className="menu-list" role="menu" aria-label="Menu">
          {SECTIONS.map((section) => {
            const actions = sections.get(section) ?? [];
            if (actions.length === 0) return null;
            return (
              <div key={section} className="menu-section" role="group" aria-label={section}>
                <div className="menu-heading" aria-hidden="true">
                  {section}
                </div>
                {actions.map((action) => {
                  at++;
                  const index = at;
                  const disabled = !runnable(action);
                  const key = action.keys[0];
                  return (
                    <button
                      key={action.id}
                      type="button"
                      role="menuitem"
                      className="menu-item"
                      aria-disabled={disabled || undefined}
                      tabIndex={index === item ? 0 : -1}
                      title={action.label && action.label !== action.name ? action.name : undefined}
                      // Only a pointer that moves takes the highlight: a menu opened
                      // from the keyboard under a resting pointer keeps its item.
                      onMouseMove={() => index !== item && setItem(index)}
                      onClick={() => run(action)}
                      onKeyDown={(event) => onItemKey(event, index)}
                    >
                      <span>{action.label ?? action.name}</span>
                      {key && <kbd>{describeKey(key, mac)}</kbd>}
                    </button>
                  );
                })}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
