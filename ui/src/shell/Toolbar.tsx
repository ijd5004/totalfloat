// The toolbar row under the title block: the layout picker, then the grid's
// filters and grouping (which the grid puts in `slot`), then the Gantt's zoom,
// the view toggles and Add activity. One quiet row; the only bold colour in
// the frame is the status bar's Apply.

import { useState } from "react";

import { type Registry, describeKey } from "../commands";
import type { ZoomChoice } from "../gantt/time";
import { ToolPopover } from "../grid/ToolPopover";
import { Icon, type IconName } from "./icons";

/** The zoom choices the toolbar offers: a name for each scale, by its bottom tier. */
export const ZOOM_CHOICES: readonly { label: string; zoom: ZoomChoice }[] = [
  { label: "Week", zoom: 3 },
  { label: "Month", zoom: 2 },
  { label: "Quarter", zoom: 0 },
  { label: "Fit", zoom: "fit" },
];

/** The layout commands, in the picker's menu. */
const LAYOUT_ITEMS = [
  { id: "layout.open", label: "Open layout…" },
  { id: "layout.save", label: "Save layout" },
  { id: "layout.save-as", label: "Save layout as…" },
  { id: "layout.delete", label: "Delete layout…" },
];

export function Toolbar({
  registry,
  mac,
  layout,
  zoom,
  onZoom,
  toggles,
  onSlot,
  onAdd,
}: {
  registry: Registry;
  mac: boolean;
  /** The layout open, or null for none. */
  layout: string | null;
  zoom: ZoomChoice;
  onZoom: (zoom: ZoomChoice) => void;
  /** The view toggles: each runs a command, shown pressed while what it shows is on. */
  toggles: readonly { command: string; icon: IconName; pressed: boolean; onToggle?: () => void }[];
  /** Hears the element the grid's filters and grouping go in. */
  onSlot: (slot: HTMLDivElement | null) => void;
  /** Adds an activity; until it's given, the button is hidden. */
  onAdd?: (() => void) | undefined;
}) {
  const [picking, setPicking] = useState(false);
  const action = (id: string) => registry.inOrder().find((a) => a.id === id);
  const run = (id: string) => {
    const found = action(id);
    if (found && (!found.enabled || found.enabled())) found.run();
  };
  return (
    <div className="toolbar">
      <ToolPopover
        label={
          <>
            <Icon name="table" size={15} />
            <span className="layout-name">{layout ?? "All activities"}</span>
            <Icon name="down" size={13} />
          </>
        }
        name={`Layout: ${layout ?? "none"}`}
        role="menu"
        open={picking}
        onOpen={setPicking}
      >
        {LAYOUT_ITEMS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            role="menuitem"
            className="tool-item"
            onClick={() => {
              setPicking(false);
              run(id);
            }}
          >
            {label}
          </button>
        ))}
      </ToolPopover>
      <span className="tool-sep" aria-hidden="true" />
      <div className="toolbar-grid" ref={onSlot} />
      <div className="toolbar-end">
        <div className="segmented" role="radiogroup" aria-label="Zoom">
          {ZOOM_CHOICES.map((choice) => (
            <button
              key={choice.label}
              type="button"
              role="radio"
              aria-checked={zoom === choice.zoom}
              onClick={() => onZoom(choice.zoom)}
            >
              {choice.label}
            </button>
          ))}
        </div>
        {toggles.map(({ command, icon, pressed, onToggle }) => {
          const found = action(command);
          if (!found) return null;
          const name = found.label ?? found.name;
          const key = found.keys[0];
          return (
            <button
              key={command}
              type="button"
              className="tool-toggle"
              aria-label={name}
              aria-pressed={pressed}
              aria-keyshortcuts={key ? describeKey(key, false) : undefined}
              title={key ? `${name} (${describeKey(key, mac)})` : name}
              onClick={() => (onToggle ? onToggle() : run(command))}
            >
              <Icon name={icon} />
            </button>
          );
        })}
        <span className="tool-sep" aria-hidden="true" hidden={!onAdd} />
        <button type="button" className="tool-button add-activity" hidden={!onAdd} title="Add activity" onClick={onAdd}>
          <Icon name="plus" size={14} />
          <span className="tool-words">Add activity</span>
        </button>
      </div>
    </div>
  );
}
