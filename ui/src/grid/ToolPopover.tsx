// A toolbar button that opens a small panel under it: a menu, or a few
// controls. A click or focus elsewhere, or Escape, closes it; Escape gives
// focus back to the button.

import { type ReactNode, useEffect, useId, useRef } from "react";

export function ToolPopover({
  label,
  name,
  title,
  role,
  open,
  onOpen,
  pressed,
  children,
}: {
  /** What the button shows. */
  label: ReactNode;
  /** The button's accessible name, where it isn't its text. */
  name?: string;
  title?: string;
  role: "menu" | "dialog";
  open: boolean;
  onOpen: (open: boolean) => void;
  /** Whether what it sets is on, for a button that shows that. */
  pressed?: boolean;
  children: ReactNode;
}) {
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();

  useEffect(() => {
    if (!open) return;
    const away = (event: Event) => {
      const target = event.target as Node;
      if (!panel.current?.contains(target) && !button.current?.contains(target)) onOpen(false);
    };
    window.addEventListener("mousedown", away);
    window.addEventListener("focusin", away);
    // Focus goes to its first control as it opens.
    panel.current?.querySelector<HTMLElement>("button:not([disabled]), select, input")?.focus();
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("focusin", away);
    };
  }, [open, onOpen]);

  return (
    <span className="tool-popover">
      <button
        ref={button}
        type="button"
        className="tool-button"
        aria-label={name}
        title={title}
        aria-haspopup={role}
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-pressed={pressed}
        onClick={() => onOpen(!open)}
      >
        {label}
      </button>
      {open && (
        <div
          ref={panel}
          id={id}
          className={`tool-panel tool-panel-${role}`}
          role={role}
          aria-label={name ?? (typeof label === "string" ? label : undefined)}
          onKeyDown={(event) => {
            // In a menu, arrows move between its items.
            if (role === "menu" && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
              event.preventDefault();
              const items = [...(panel.current?.querySelectorAll<HTMLElement>('[role^="menuitem"]') ?? [])];
              const at = items.indexOf(document.activeElement as HTMLElement);
              const down = event.key === "ArrowDown";
              const to = at < 0 ? (down ? 0 : items.length - 1) : (at + (down ? 1 : -1) + items.length) % items.length;
              items[to]?.focus();
              return;
            }
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              onOpen(false);
              button.current?.focus();
            }
          }}
        >
          {children}
        </div>
      )}
    </span>
  );
}
