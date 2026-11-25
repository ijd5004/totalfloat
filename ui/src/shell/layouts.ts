// A layout file (`totalfloat.layout/1`) to and from what the app shows: the
// grid's columns, sorting, grouping (at every level) and filters, the Gantt's zoom, logic
// lines and today line, and the split between them.

import type { Layout } from "../api";
import type { LineMode } from "../gantt/draw";
import { ZOOMS, type ZoomChoice, nearestLevel } from "../gantt/time";
import type { ColumnId } from "../grid/columns";
import { builtOf, conditionsOf } from "../grid/filters";
import type { GridLayout } from "../grid/Grid";

/** What a layout sets beyond the grid. */
export interface Setting {
  grid: GridLayout;
  zoom: ZoomChoice;
  lines: LineMode;
  today: boolean;
  split: number;
}

type Field = Layout["columns"][number]["field"];

/** A column the layout format names: every grid column but Before and After. */
function isField(id: ColumnId): id is Field {
  return id !== "before" && id !== "after";
}

/** The layout for what's shown now. */
export function layoutOf(setting: Setting): Layout {
  const zoom = setting.zoom === "fit" ? null : (ZOOMS[setting.zoom] ?? ZOOMS[0]);
  const shown = setting.grid.columns
    .filter((c): c is { field: Field; width: number } => isField(c.field))
    .map((c) => ({ field: c.field, width: Math.min(2000, Math.max(40, Math.round(c.width))) }));
  // A layout shows at least one column.
  const [first = { field: "code" as const, width: 110 }, ...rest] = shown;
  return {
    schema: "totalfloat.layout/1",
    columns: [first, ...rest],
    sort: setting.grid.sort.filter((k) => isField(k.column)).map((k) => ({ field: k.column as Field, descending: k.descending })),
    group: setting.grid.grouped ? (setting.grid.groupBy ?? "wbs") : "none",
    // Left out when there are none, so a layout with one level is written as before.
    ...(setting.grid.grouped && setting.grid.thenBy && setting.grid.thenBy.length > 0
      ? { then_by: setting.grid.thenBy.slice(0, 3) as NonNullable<Layout["then_by"]> }
      : {}),
    filters: {
      text: setting.grid.text,
      quick: [...setting.grid.quick],
      // Each left out when empty, so a layout without one is written as before.
      ...layoutBuilt(setting.grid.built),
    },
    gantt: {
      zoom: zoom ? { px_per_day: zoom.pxPerDay, top: zoom.top, bottom: zoom.bottom } : "fit",
      lines: setting.lines,
      today: setting.today,
    },
    split: Math.round(Math.min(0.85, Math.max(0.15, setting.split)) * 1000) / 1000,
  };
}

/** A layout's `built` and `any`, each only when it holds something. */
function layoutBuilt(conditions: Setting["grid"]["built"]) {
  const { built, any } = builtOf(conditions);
  return {
    ...(Object.keys(built).length > 0 ? { built } : {}),
    ...(any.length > 0 ? { any } : {}),
  };
}

/** What a layout sets the app to. */
export function settingOf(layout: Layout): Setting {
  return {
    grid: {
      columns: layout.columns.map((c) => ({ field: c.field, width: c.width })),
      sort: (layout.sort ?? []).map((k) => ({ column: k.field, descending: k.descending ?? false })),
      grouped: layout.group !== "none",
      groupBy: layout.group === "wbs" || layout.group === "none" ? null : layout.group,
      ...(layout.group !== "none" && layout.then_by && layout.then_by.length > 0 ? { thenBy: [...layout.then_by] } : {}),
      text: layout.filters?.text ?? "",
      quick: layout.filters?.quick ?? [],
      built: conditionsOf(layout.filters?.built ?? {}, layout.filters?.any ?? []),
    },
    zoom: layout.gantt.zoom === "fit" ? "fit" : nearestLevel(layout.gantt.zoom.px_per_day),
    lines: layout.gantt.lines,
    today: layout.gantt.today ?? false,
    split: layout.split,
  };
}
