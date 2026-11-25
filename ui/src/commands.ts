// Every action is registered once, with a name, its keys and a handler, so
// the command palette, the key list and the keyboard can't disagree.

/** A key as written in a binding: `mod+k`, `?`, `alt+r`, `escape`. `mod` is Ctrl, or Cmd on a Mac. */
export type Key = string;

export interface Action {
  /** Stable, for tests and settings: `palette.open`. */
  id: string;
  /** What the palette and key list show. */
  name: string;
  /** What the Menu shows, where shorter than the name: "Export XER". */
  label?: string;
  /** Where the key list groups it. */
  group: string;
  keys: Key[];
  run: () => void;
  /** Whether it can run now; the palette leaves out what can't. */
  enabled?: () => boolean;
  /** Whether the palette lists it; the action that opens the palette doesn't. */
  inPalette?: boolean;
  /**
   * Whether its keys work while typing in a text field. Actions on text
   * editing keys, such as Ctrl+Z, leave them to the field.
   */
  inText?: boolean;
}

/** The parts of a keyboard event a binding is matched on. */
export interface Pressed {
  key: string;
  /** The physical key, `KeyR` or `Digit1`, where known. */
  code?: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

/**
 * The binding a key press makes. A single printable character stands for
 * itself, so `?` matches whatever keyboard makes it; Shift counts only with
 * named keys and with Ctrl, Cmd or Alt. With Ctrl, Cmd or Alt, a letter or
 * digit is the one the layout gives, as people know their shortcuts by
 * letter. Only when the modifier changed the character, as Option does on a
 * Mac (Option+R types `®`) and AltGr elsewhere, is it read from the physical
 * key instead, for letters, digits and the `=` and `-` keys.
 */
export function bindingOf(pressed: Pressed): Key {
  const modified = pressed.ctrlKey || pressed.metaKey || pressed.altKey;
  const parts: string[] = [];
  if (pressed.ctrlKey || pressed.metaKey) parts.push("mod");
  if (pressed.altKey) parts.push("alt");
  if (pressed.shiftKey && (modified || pressed.key.length > 1)) parts.push("shift");
  const logical = /^[a-z0-9]$/i.test(pressed.key);
  const physical = /^(?:Key([A-Z])|Digit([0-9])|(Equal|Minus))$/.exec(pressed.code ?? "");
  if (modified && !logical && pressed.key.length === 1 && physical) {
    const sign = physical[3] === "Equal" ? "=" : physical[3] === "Minus" ? "-" : undefined;
    parts.push(sign ?? (physical[1] ?? physical[2] ?? "").toLowerCase());
  } else {
    parts.push(pressed.key.length === 1 && !modified ? pressed.key : pressed.key.toLowerCase());
  }
  return parts.join("+");
}

/** A binding as a person reads it: `Ctrl+K`, or `⌘K` on a Mac. */
export function describeKey(key: Key, mac: boolean): string {
  // The plus key names itself; it can't be split on.
  if (key === "+") return "+";
  const names: Record<string, string> = mac
    ? { mod: "⌘", alt: "⌥", shift: "⇧" }
    : { mod: "Ctrl", alt: "Alt", shift: "Shift" };
  const parts = key.split("+").map((part) => {
    const named = names[part];
    if (named) return named;
    if (part === "escape") return "Esc";
    return part.length === 1 ? part.toUpperCase() : part[0]?.toUpperCase() + part.slice(1);
  });
  return parts.join(mac ? "" : "+");
}

/** The actions, by id. */
export class Registry {
  private readonly actions = new Map<string, Action>();

  /** Adds `action`; returns what removes it. A second action with the same id or key is a bug. */
  register(action: Action): () => void {
    if (this.actions.has(action.id)) throw new Error(`action ${action.id} is registered twice`);
    for (const key of action.keys) {
      const taken = this.forKey(key);
      if (taken) throw new Error(`${key} is bound to both ${taken.id} and ${action.id}`);
    }
    this.actions.set(action.id, action);
    return () => {
      if (this.actions.get(action.id) === action) this.actions.delete(action.id);
    };
  }

  /** Every action, in the order registered, as menus list them. */
  inOrder(): Action[] {
    return [...this.actions.values()];
  }

  /** Every action, by group then name. */
  all(): Action[] {
    return [...this.actions.values()].sort(
      (a, b) => a.group.localeCompare(b.group) || a.name.localeCompare(b.name),
    );
  }

  forKey(key: Key): Action | undefined {
    for (const action of this.actions.values()) {
      if (action.keys.includes(key)) return action;
    }
    return undefined;
  }

  /** The palette's actions that can run now whose names hold every word of `query`, in any case. */
  search(query: string): Action[] {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return this.all().filter(
      (action) =>
        (action.inPalette ?? true) &&
        (action.enabled?.() ?? true) &&
        words.every((word) => action.name.toLowerCase().includes(word)),
    );
  }

  /** Runs the action bound to a key press, if any can run; says whether one did. */
  press(pressed: Pressed): boolean {
    const action = this.forKey(bindingOf(pressed));
    if (!action || !(action.enabled?.() ?? true)) return false;
    action.run();
    return true;
  }
}

/** Whether keys typed here are text, so only bindings with Ctrl, Cmd or Alt, and Escape, apply. */
export function typingIn(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

/**
 * Whether a key press made in `target` is the app's to act on: anywhere but a
 * text field, and in one only with Ctrl, Cmd or Alt, and then not for actions
 * on the field's own editing keys, such as Ctrl+Z.
 */
export function appTakes(registry: Registry, pressed: Pressed, target: EventTarget | null): boolean {
  // Keys pressed in the menu are the menu's, bar the modified ones.
  const modified = pressed.ctrlKey || pressed.metaKey || pressed.altKey;
  if (!modified && target instanceof Element && target.closest('[role="menu"]')) return false;
  if (!typingIn(target)) return true;
  if (!(pressed.ctrlKey || pressed.metaKey || pressed.altKey)) return false;
  return registry.forKey(bindingOf(pressed))?.inText !== false;
}
