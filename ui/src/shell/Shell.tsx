import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";

import { type Api, ApiError, type Explanation, type Fields, type Layout, type Links, type ProjectWbs, type Rows, type Summary, type Wbs, failure } from "../api";
import { type Registry, appTakes, bindingOf } from "../commands";
import { ChangesPanel } from "../edit/ChangesPanel";
import { type Change, Editing, changesWaiting, latest as latestChanges, wbsNamed } from "../edit/editing";
import { type Connection, EventFeed } from "../events";
import type { DrivingPath, LineMode } from "../gantt/draw";
import { type Drawing, Gantt } from "../gantt/Gantt";
import { DEFAULT_ZOOM, type ZoomChoice, stepZoom } from "../gantt/time";
import { Grid, type GridActions, type GridEditing, type GridLayout } from "../grid/Grid";
import { DeleteDialog, type Deleting } from "./DeleteDialog";
import { finderOf } from "./finder";
import type { Band } from "../grid/model";
import { type PrintColumn, PrintView } from "../print/PrintView";
import { COLUMNS, COMPARE_COLUMNS, type Compared, type Units, showUnits } from "../grid/columns";
import { type WbsRef, wbsNames, wbsOrder, wbsRefs } from "../grid/model";
import { type NewActivity, addEdit, changeKey, rowsWith, wbsProblem, wbsWith } from "../edit/shape";
import { WbsPicker, wbsChoices } from "./WbsPicker";
import { nextId } from "../edit/ids";
import { DATE_STYLES, type DateFormat, DEFAULT_DATES, isDateStyle, showDates } from "../dates";
import { download } from "../history/download";
import { HistoryPanel } from "../history/HistoryPanel";
import { Inspector, type InspectorEditing } from "../inspector/Inspector";
import { addKey, addLink, isLinkKey, linkKey } from "../inspector/linkEdits";
import { NetworkPanel } from "../network/NetworkPanel";
import { RiskPanel, riskMemory, scheduleIdentity } from "../risk/RiskPanel";
import { UsagePanel } from "../usage/UsagePanel";
import { ProjectsPanel } from "../portfolio/ProjectsPanel";
import { ProjectNetwork } from "../network/ProjectNetwork";
import { MergeDialog } from "../history/MergeDialog";
import type { Say } from "../history/words";
import { View } from "../view";
import { EditBar, type Outlook, StatusBar, TitleBlock } from "./Bars";
import { Icon } from "./icons";
import { MenuButton } from "./MenuButton";
import { Toolbar } from "./Toolbar";
import { ProjectPicker } from "./ProjectPicker";
import { outlookOf } from "./outlook";
import { type Proposal, ReviewPanel } from "../review/ReviewPanel";
import { BranchPicker } from "./BranchPicker";
import { LayoutDialog, type LayoutMode } from "./LayoutDialog";
import { layoutOf, settingOf } from "./layouts";
import { switchBranch } from "./switch";
import { KeyList } from "./KeyList";
import { Palette } from "./Palette";

/** What the shell has fetched. */
interface Shown {
  summary: Summary | null;
  rows: Rows | null;
  links: Links | null;
  /** Each WBS node's place in the WBS, by path. */
  order: Map<string, number> | null;
  /** Each named WBS node's name, by path. */
  names: Map<string, string> | null;
  /** Each WBS node's `wbs_id`, by path, for edits that name a node. */
  /** How an edit names each WBS node, by its dotted path. */
  refs: Map<string, WbsRef> | null;
  /** Every activity code type and user field, for filtering and grouping; null without them. */
  fields: Fields | null;
}

const NOTHING: Shown = { summary: null, rows: null, links: null, order: null, names: null, refs: null, fields: null };

type Overlay = "palette" | "keys" | "layouts" | null;

/** Where the layout last used on a schedule is remembered. */
function layoutKey(location: string): string {
  return `totalfloat.layout:${location}`;
}

/** What the schedule's right column shows, one at a time. */
type Panel = "changes" | "inspector" | "network" | "review";

/** Where what's shown is saved, for the status bar. */
function savedWords(summary: Summary | null): string | null {
  if (!summary) return null;
  if (summary.branch) return `Saved on ${summary.branch}`;
  return summary.head ? "A fixed point in history" : "Plain file, read-only";
}

/** Each panel's name, for the right column's label. */
const PANEL_NAMES: Record<Panel, string> = {
  changes: "Changes",
  inspector: "Inspector",
  network: "Network around the selection",
  review: "Review",
};

/** The top-level views, each over the whole work area. */
type Page = "schedule" | "network" | "resources" | "risk" | "history";

/** The views, as the title block lists them, with their commands' words and keys. */
const VIEWS: { id: Page; command: string; label: string; name: string; keys: string[] }[] = [
  { id: "schedule", command: "view.schedule", label: "Schedule", name: "Schedule: the grid, Gantt and right column", keys: ["alt+shift+g"] },
  { id: "network", command: "view.project-network", label: "Network", name: "Network: the whole project's activity network", keys: ["alt+shift+w"] },
  { id: "resources", command: "view.usage", label: "Resources", name: "Resources: units per week or month against the limit", keys: ["alt+shift+u"] },
  { id: "risk", command: "view.risk", label: "Risk", name: "Risk: the odds of each finish date", keys: ["alt+shift+r"] },
  { id: "history", command: "view.history", label: "History", name: "History: changes, branches and baselines", keys: ["alt+h"] },
];

/** Everything the shell shows, or why it couldn't be fetched. */
async function fetchShown(api: Api): Promise<Shown | ApiError> {
  try {
    const summary = await api.summary();
    // The WBS tree only orders the bands; without it they fall back to code order.
    const wbs = api.wbs().catch(() => null);
    // Without them the grid still shows, with no filtering or grouping by codes and fields.
    const fields = api.fields().catch(() => null);
    const [rows, links, tree, found] = await Promise.all([api.rows(), api.links(), wbs, fields]);
    return { summary, rows, links, order: orderOf(tree), names: namesOf(tree), refs: refsOf(tree), fields: found };
  } catch (error) {
    return error instanceof ApiError
      ? error
      : new ApiError(0, "unreachable", "The server couldn't be reached.", "Is `totalfloat serve` still running?");
  }
}

/** The WBS's order, or null for code order if the tree is missing or malformed. */
export function orderOf(tree: Wbs | null): Map<string, number> | null {
  const projects: unknown = tree?.projects;
  if (!Array.isArray(projects)) return null;
  try {
    return wbsOrder(projects as ProjectWbs[]);
  } catch {
    return null;
  }
}

/** How an edit names each WBS node, or null if the tree is missing or malformed. */
export function refsOf(tree: Wbs | null): Map<string, WbsRef> | null {
  const projects: unknown = tree?.projects;
  if (!Array.isArray(projects)) return null;
  try {
    return wbsRefs(projects as ProjectWbs[]);
  } catch {
    return null;
  }
}

/** The WBS nodes' names, or null if the tree is missing or malformed. */
export function namesOf(tree: Wbs | null): Map<string, string> | null {
  const projects: unknown = tree?.projects;
  if (!Array.isArray(projects)) return null;
  try {
    return wbsNames(projects as ProjectWbs[]);
  } catch {
    return null;
  }
}

/**
 * The driving path explain gives: each chain's activities, and the
 * relationships between consecutive steps, which are what drove each.
 */
export function pathOf(report: Explanation, from: number): DrivingPath {
  const ids = new Set<number>([from]);
  const relationships = new Set<number>();
  const edges = new Set<string>();
  for (const chain of report.chains) {
    chain.steps.forEach((step, k) => {
      ids.add(step.activity);
      // A step a relationship drove names it; others run from the next step,
      // as steps go from the date back to its origin.
      if (step.relationship != null) {
        relationships.add(step.relationship);
        return;
      }
      const back = chain.steps[k + 1];
      if (back && back.activity !== step.activity) edges.add(`${back.activity}>${step.activity}`);
    });
  }
  return { ids, relationships, edges };
}

/**
 * What the Gantt lights: a driving path asked for, unless an opened
 * proposal's changes are lit; otherwise the inspector's explain chain, which
 * follows every selection, or the proposal's changes.
 */
export function litOf(path: DrivingPath | null, highlight: DrivingPath | null, chain: DrivingPath | null): DrivingPath | null {
  return path ? (highlight ?? path) : (chain ?? highlight);
}

/** Saves what's shown as an XER file, through the browser's download. */
async function exportXer(api: Api, editing: Editing): Promise<void> {
  try {
    const { name, blob } = await api.exportXer();
    download(name, blob);
    editing.say(`Exported ${name}.`, "done");
  } catch (error) {
    const { message, hint } = failure(error, "The schedule couldn't be exported.");
    editing.say(message, "bad", hint);
  }
}

/** A project to ask about, when there is one. */
function projectOf(project: number | undefined): { project?: number } {
  return project === undefined ? {} : { project };
}

/** What editing needs of what's shown. */
function editable(shown: Shown) {
  return { rows: shown.rows, head: shown.summary?.head ?? null, writable: shown.summary?.writable ?? false };
}

const SPLIT_SETTING = "totalfloat.split";
const UNITS_SETTING = "totalfloat.units";
const DATES_SETTING = "totalfloat.dates";

/** The date format chosen before, or the default. */
function storedDates(): DateFormat {
  try {
    const kept: unknown = JSON.parse(localStorage.getItem(DATES_SETTING) ?? "null");
    if (kept && typeof kept === "object" && "style" in kept && isDateStyle(kept.style)) {
      return { style: kept.style, time: "time" in kept && kept.time === true };
    }
  } catch {
    // No storage, or something else in it; the default stands.
  }
  return DEFAULT_DATES;
}

/** Days unless hours were chosen before. */
function storedUnits(): Units {
  try {
    return localStorage.getItem(UNITS_SETTING) === "hours" ? "hours" : "days";
  } catch {
    return "days";
  }
}

/** The grid's share of the width, as kept from last time. */
function storedSplit(): number {
  try {
    const kept = Number(localStorage.getItem(SPLIT_SETTING));
    if (kept >= 0.15 && kept <= 0.85) return kept;
  } catch {
    // No storage; the default stands.
  }
  return 0.6;
}

/** Keeps the split for next time. */
function keepSplit(split: number): void {
  try {
    localStorage.setItem(SPLIT_SETTING, String(split));
  } catch {
    // Storage can be off; the split just isn't kept.
  }
}

/**
 * The bar between the grid and the Gantt: drag it, or use the arrow keys, to
 * share the width. `onDone` hears the split once a drag or key press ends.
 */
export function Splitter({
  split,
  onSplit,
  onDone,
}: {
  split: number;
  onSplit: (split: number) => void;
  onDone: (split: number) => void;
}) {
  const clamp = (value: number) => Math.min(0.85, Math.max(0.15, value));
  return (
    <div
      className="splitter"
      role="separator"
      aria-orientation="vertical"
      aria-label="Grid and Gantt widths"
      aria-valuemin={15}
      aria-valuemax={85}
      aria-valuenow={Math.round(split * 100)}
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        const next = clamp(split + (event.key === "ArrowLeft" ? -0.05 : 0.05));
        onSplit(next);
        onDone(next);
      }}
      onPointerDown={(event) => {
        const bar = event.currentTarget;
        const views = bar.parentElement;
        if (!views || event.button !== 0) return;
        event.preventDefault();
        // The bar keeps the pointer until it's let go or the browser takes it back.
        bar.setPointerCapture?.(event.pointerId);
        const box = views.getBoundingClientRect();
        let last = split;
        const move = (e: PointerEvent) => {
          last = clamp((e.clientX - box.left) / box.width);
          onSplit(last);
        };
        const end = () => {
          bar.removeEventListener("pointermove", move);
          bar.removeEventListener("pointerup", end);
          bar.removeEventListener("pointercancel", end);
          onDone(last);
        };
        bar.addEventListener("pointermove", move);
        bar.addEventListener("pointerup", end);
        bar.addEventListener("pointercancel", end);
      }}
    />
  );
}

/** Whether this is a Mac, where `mod` is Cmd. */
export function onMac(): boolean {
  return /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
}

/**
 * The frame every screen sits in: the top bar, the grid and Gantt, a side
 * panel, the status bar, and the command palette over them.
 */
export function Shell({ api, registry, follow = true }: { api: Api; registry: Registry; follow?: boolean }) {
  const [shown, setShown] = useState<Shown>(NOTHING);
  const [failed, setFailed] = useState<ApiError | null>(null);
  const [connection, setConnection] = useState<Connection>("connecting");
  const [overlay, setOverlay] = useState<Overlay>(null);
  const [panel, setPanel] = useState<Panel | null>(null);
  // The view over the work area. The schedule stays laid out under the
  // others, so it comes back with its scroll and selection as they were.
  const [page, setPage] = useState<Page>("schedule");
  const pageNow = useRef(page);
  pageNow.current = page;
  // History stays once opened, hidden while another view shows, so a
  // comparison it shows in the grid lasts while the schedule is looked at.
  // Risk keeps its form and results in `risk` instead.
  const [historyKept, setHistoryKept] = useState(false);
  if (page === "history" && !historyKept) setHistoryKept(true);
  // A view shown takes the focus the one before had, which `inert` takes
  // away: the grid, coming back to the schedule, or the view.
  const firstPage = useRef(true);
  useEffect(() => {
    if (firstPage.current) {
      firstPage.current = false;
      return;
    }
    const at = document.activeElement;
    if (at instanceof HTMLElement && at !== document.body && !at.closest("[inert], [hidden]")) return;
    const to = page === "schedule" ? document.querySelector<HTMLElement>(".grid-body") : document.getElementById(`view-${page}`);
    to?.focus();
  }, [page]);
  // Whether the project picker is open.
  const [picking, setPicking] = useState(false);
  // The risk view's form and results, kept while it's closed.
  const risk = useRef(riskMemory());
  // The right column's toggle opens the panel last shown.
  const lastPanel = useRef<Panel>("inspector");
  if (panel) lastPanel.current = panel;
  /** Shows a panel in the schedule's right column; again, from the schedule, closes it. */
  const showPanel = useCallback((id: Panel) => {
    if (pageNow.current !== "schedule") {
      setPage("schedule");
      setPanel(id);
    } else setPanel((open) => (open === id ? null : id));
  }, []);
  // Where the grid puts its filters and grouping (in the toolbar row under
  // the title block) and its counts (in the status bar).
  const [toolbar, setToolbar] = useState<HTMLDivElement | null>(null);
  const [statusSlot, setStatusSlot] = useState<HTMLSpanElement | null>(null);
  // When the schedule shown was last calculated and fetched, for the status bar.
  const [calculatedAt, setCalculatedAt] = useState<Date | null>(null);
  // A comparison the history panel shows in the grid's Before and After columns.
  const [compared, setCompared] = useState<Compared | null>(null);
  // An opened proposal's changed activities, lit in the Gantt.
  const [highlight, setHighlight] = useState<DrivingPath | null>(null);
  // The branch being merged into the one shown, while its dialog is open.
  const [merging, setMerging] = useState<string | null>(null);
  // What the grid can do for the frame: add where the selection is, say what's selected.
  const gridActions = useRef<GridActions | null>(null);
  // Activities about to be deleted, once the dry run has counted what goes with them.
  const [deleting, setDeleting] = useState<Deleting | null>(null);
  // The activities asked to move, while the picker asks where to.
  const [moving, setMoving] = useState<{ codes: string[]; project: number | undefined } | null>(null);
  // Asks where the selected activities move to; kept current for the command.
  const startMove = useRef<() => void>(() => {});
  // Deletes the selected activities, after asking; kept current for the command.
  const startDelete = useRef<() => Promise<void>>(async () => {});
  // What's printed: the grid's lines and columns as they were when Print was chosen.
  const [printing, setPrinting] = useState<{
    rows: Rows;
    lines: Int32Array;
    bands: Band[];
    columns: PrintColumn[];
    summary: Summary | null;
  } | null>(null);
  const closePrint = useCallback(() => setPrinting(null), []);
  // The summary now, for a print to take as it starts.
  const settingsSummary = useRef<Summary | null>(null);
  useEffect(() => {
    settingsSummary.current = shown.summary;
  }, [shown.summary]);
  // The proposals waiting for review, kept current by the change feed.
  const [proposals, setProposals] = useState<Proposal[] | { message: string; hint: string } | null>(null);
  const proposalsAsked = useRef(0);
  // Whether a history store is shown: a plain file has no proposals to list.
  const isStore = useRef(false);
  const listProposals = useCallback(() => {
    const ticket = ++proposalsAsked.current;
    void api.proposals().then(
      (answer) => ticket === proposalsAsked.current && setProposals(answer.proposals),
      (error: unknown) => ticket === proposalsAsked.current && setProposals(failure(error, "The proposals couldn't be listed.")),
    );
  }, [api]);
  const mac = useMemo(() => onMac(), []);
  const filter = useRef<HTMLInputElement>(null);
  // The grid and Gantt share one view of the rows, and split the width.
  const view = useMemo(() => new View(), []);
  const [split, setSplit] = useState(() => storedSplit());
  const [zoom, setZoom] = useState<ZoomChoice>(DEFAULT_ZOOM);
  // The pixels a day the Gantt last fitted to, so a zoom step from fit goes to the nearest level.
  const fitted = useRef(8);
  const onFitted = useCallback((pxPerDay: number) => {
    fitted.current = pxPerDay;
  }, []);
  const [today, setToday] = useState(false);
  // Durations and float in days, or hours; kept between visits.
  const [units, setUnits] = useState<Units>(() => storedUnits());
  // Set as the shell renders, before the grid and Gantt do, so their text
  // and titles are in the units chosen in the same render.
  showUnits(units);
  useEffect(() => {
    try {
      localStorage.setItem(UNITS_SETTING, units);
    } catch {
      // Storage can be off; the choice just isn't kept.
    }
  }, [units]);
  // How dates are written, kept between visits; set as the shell renders, as units are.
  const [dates, setDates] = useState<DateFormat>(() => storedDates());
  showDates(dates);
  useEffect(() => {
    try {
      localStorage.setItem(DATES_SETTING, JSON.stringify(dates));
    } catch {
      // Storage can be off; the choice just isn't kept.
    }
  }, [dates]);
  // Whether what's shown can be exported: a point in a history store.
  const exportable = useRef(false);
  useEffect(() => {
    exportable.current = Boolean(shown.summary?.head);
  }, [shown.summary]);
  const [lines, setLines] = useState<LineMode>("all");
  // The inspector's explain chain, which the Gantt lights unless a driving path was asked for (`litOf`).
  const [chain, setChain] = useState<DrivingPath | null>(null);
  // Bumped by the Explain command, to bring the inspector's "Why" into view,
  // and by the inspector's key, to move into it; acted on once it's shown.
  const [whyAsked, setWhyAsked] = useState(0);
  const [inspectorAsked, setInspectorAsked] = useState(0);
  // Asked by Alt+B and Alt+A: the inspector's Add predecessor or Add successor field.
  const [addAsked, setAddAsked] = useState<{ side: "predecessor" | "successor"; n: number } | null>(null);
  useEffect(() => {
    if (whyAsked > 0) document.getElementById("inspector-why")?.scrollIntoView({ block: "start" });
  }, [whyAsked]);
  useEffect(() => {
    if (inspectorAsked > 0) document.querySelector<HTMLElement>(".right-column .inspector")?.focus();
  }, [inspectorAsked]);

  // Each load is numbered; only the latest is shown, so a slow, older fetch
  // can't overwrite a newer one, and none lands after the shell is gone.
  // The count only rises, even when StrictMode replays effects.
  const latest = useRef(0);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // The latest load, so a caller can wait for what ends up shown.
  const inflight = useRef<Promise<Shown | null>>(Promise.resolve(null));

  /** Fetches everything shown again, and shows it; resolves with it, or null if it isn't shown. */
  const load = useCallback((): Promise<Shown | null> => {
    const ticket = ++latest.current;
    const loading = fetchShown(api).then((result) => {
      if (!mounted.current || ticket !== latest.current) return null;
      if (result instanceof ApiError) {
        setFailed(result);
        return null;
      }
      setShown(result);
      setFailed(null);
      setCalculatedAt(new Date());
      return result;
    });
    inflight.current = loading;
    return loading;
  }, [api]);

  /**
   * Loads, and resolves with what ends up shown: if a newer load overtakes
   * this one, as the live feed's can after a change, it waits for that.
   */
  const loadShown = useCallback(async (): Promise<Shown | null> => {
    let loading = load();
    for (;;) {
      const result = await loading;
      if (loading === inflight.current || !mounted.current) return result;
      loading = inflight.current;
    }
  }, [load]);

  useEffect(() => {
    void load();
  }, [load]);

  // What waits for review, once a history store is shown and whenever its head moves.
  const storeHead = shown.summary?.head ?? null;
  useEffect(() => {
    isStore.current = storeHead !== null;
    if (storeHead) listProposals();
  }, [storeHead, listProposals]);

  const editing = useMemo(() => new Editing(api), [api]);
  useEffect(() => {
    if (!addAsked) return;
    const field = document.getElementById(`inspector-add-${addAsked.side}`);
    if (field) {
      field.focus();
      field.scrollIntoView({ block: "nearest" });
    } else editing.say("Select an activity in a schedule you can edit to link it.");
  }, [addAsked, editing]);
  const say = useCallback<Say>((text, tone, hint) => editing.say(text, tone, hint), [editing]);
  /** Switches the store's checked-out branch, and shows it if this page follows it. */
  const switchTo = useCallback(
    (name: string) => switchBranch(api, editing, name, loadShown, say),
    [api, editing, loadShown, say],
  );

  // Layouts: the grid's as it reports it, the one given to it to apply, and
  // the name of the one in use.
  const gridNow = useRef<GridLayout | null>(null);
  const onGridLayout = useCallback((layout: GridLayout) => {
    gridNow.current = layout;
  }, []);
  const [gridLayout, setGridLayout] = useState<GridLayout | null>(null);
  const [layoutName, setLayoutName] = useState<string | null>(null);
  const [layoutMode, setLayoutMode] = useState<LayoutMode>("open");
  // What the commands read: the settings as they are now.
  const settings = useRef({ zoom, lines, today, split, layoutName, location: "" });
  useEffect(() => {
    settings.current = { zoom, lines, today, split, layoutName, location: shown.summary?.location ?? "" };
  }, [zoom, lines, today, split, layoutName, shown.summary]);
  const rememberLayout = useCallback((name: string | null) => {
    try {
      const key = layoutKey(settings.current.location);
      if (name) localStorage.setItem(key, name);
      else localStorage.removeItem(key);
    } catch {
      // Storage can be off; it's just not remembered.
    }
  }, []);
  const applyLayout = useCallback((layout: Layout) => {
    const setting = settingOf(layout);
    setGridLayout(setting.grid);
    setZoom(setting.zoom);
    setLines(setting.lines);
    setToday(setting.today);
    setSplit(setting.split);
    keepSplit(setting.split);
  }, []);
  // Bumped by each layout the person saves or opens, so a restore still on
  // its way when they do is dropped.
  const layoutChosen = useRef(0);
  const saveLayout = useCallback(
    async (name: string) => {
      const chosen = ++layoutChosen.current;
      const { zoom: z, lines: l, today: t, split: sp } = settings.current;
      if (!gridNow.current) return;
      await api.saveLayout(name, layoutOf({ grid: gridNow.current, zoom: z, lines: l, today: t, split: sp }));
      editing.say(`Saved layout ${name}.`, "done");
      // Another layout chosen while it saved is the one in use.
      if (chosen !== layoutChosen.current) return;
      setLayoutName(name);
      rememberLayout(name);
    },
    [api, editing, rememberLayout],
  );
  const openLayout = useCallback(
    async (name: string) => {
      const chosen = ++layoutChosen.current;
      const layout = await api.layout(name);
      if (chosen !== layoutChosen.current) return;
      applyLayout(layout);
      setLayoutName(name);
      rememberLayout(name);
      editing.say(`Opened layout ${name}.`, "done");
    },
    [api, applyLayout, editing, rememberLayout],
  );
  // The layout last used on this schedule, once it's known which schedule that is.
  const restoredFor = useRef<string | null>(null);
  const location = shown.summary?.location ?? null;
  useEffect(() => {
    if (!location || restoredFor.current === location) return;
    restoredFor.current = location;
    let name: string | null = null;
    try {
      name = localStorage.getItem(layoutKey(location));
    } catch {
      // No storage: nothing remembered.
    }
    if (!name) return;
    const remembered = name;
    const chosen = layoutChosen.current;
    // How the view is set out now: if the person changes it before the
    // layout arrives, their change stands.
    const viewNow = () => {
      const { zoom: z, lines: l, today: t, split: sp } = settings.current;
      return JSON.stringify({ grid: gridNow.current, z, l, t, sp });
    };
    const seen = viewNow();
    void api.layout(remembered).then(
      (layout) => {
        // The person has chosen, deleted or changed a layout meanwhile: theirs stands.
        if (chosen !== layoutChosen.current || viewNow() !== seen) return;
        applyLayout(layout);
        setLayoutName(remembered);
      },
      () => {
        // Deleted or unreadable since: start as usual.
      },
    );
  }, [api, applyLayout, location]);
  // The highlighted driving path: the activity asked about, and the chain back from it.
  const [path, setPath] = useState<DrivingPath | null>(null);
  const pathAsked = useRef(0);
  const showPath = useCallback(async () => {
    if (path) {
      pathAsked.current++;
      setPath(null);
      editing.say("Driving path cleared.");
      return;
    }
    const now = view.current();
    const i = now ? now.arranged.lines[now.active] : undefined;
    if (!now || i === undefined || i < 0) {
      editing.say("Select an activity first.");
      return;
    }
    const code = now.rows.code[i] ?? "";
    // Only the latest request counts, and only for the rows it was asked of.
    const ticket = ++pathAsked.current;
    try {
      const report = await api.explain(code, "early_start", projectOf(now.rows.project[i]));
      if (ticket !== pathAsked.current || view.current()?.rows !== now.rows) return;
      setPath(pathOf(report, now.rows.id[i] ?? -1));
      const [traced] = report.chains;
      const size = pathOf(report, now.rows.id[i] ?? -1).ids.size;
      editing.say(`Driving path to ${code}: ${size} activities, from ${traced?.origin ? `the ${traced.origin.replace(/-/g, " ")}` : "its start"}.`);
    } catch (error) {
      if (ticket !== pathAsked.current) return;
      const { message, hint } = failure(error, "The driving path couldn't be fetched.");
      editing.say(message, "bad", hint);
    }
  }, [api, editing, path, view]);
  // A new schedule has new paths; the highlight is for the one it was asked of.
  const [pathRows, setPathRows] = useState(shown.rows);
  if (pathRows !== shown.rows) {
    setPathRows(shown.rows);
    if (path) setPath(null);
  }
  useEffect(() => editing.reloadWith(() => loadShown().then((loaded) => loaded && editable(loaded))), [editing, loadShown]);
  useEffect(() => editing.show(editable(shown)), [editing, shown]);
  const edit = useSyncExternalStore(editing.subscribe, editing.state);
  // The first edit waiting opens the Changes panel when the right column is
  // closed, without taking the grid's focus. Another panel open stays, and
  // the status bar counts the edits instead. With previews off each commit
  // records at once, so it doesn't open. Once the edits are applied or
  // discarded, a panel that opened by itself closes again, and the right
  // column's toggle keeps the panel chosen before it.
  const panelNow = useRef(panel);
  useEffect(() => {
    panelNow.current = panel;
  }, [panel]);
  const openedOver = useRef<Panel | null>(null);
  useEffect(() => {
    let had = editing.state().pending !== null;
    return editing.subscribe(() => {
      const { pending, previewing } = editing.state();
      const has = pending !== null;
      if (has && !had && previewing && panelNow.current === null) {
        openedOver.current = lastPanel.current;
        setPanel("changes");
      } else if (!has && had && openedOver.current) {
        const last = openedOver.current;
        openedOver.current = null;
        if (panelNow.current === "changes") {
          lastPanel.current = last;
          setPanel(null);
        }
      }
      had = has;
    });
  }, [editing]);
  const fields = shown.fields;
  const finder = useMemo(() => (fields ? finderOf(api, fields) : undefined), [api, fields]);
  // The rows with the changes waiting: activities added show, those deleted go.
  const bare = units === "days" ? "d" : "h";
  // On the changes alone, so a preview's busy and notice don't give the grid new rows.
  const changesShown = edit.pending?.changes;
  const queuedShown = edit.queued;
  const changesAll = useMemo(
    () => (queuedShown.length > 0 ? latestChanges([...(changesShown ?? []), ...queuedShown]) : changesShown),
    [changesShown, queuedShown],
  );
  const rowsShown = useMemo(() => (shown.rows ? rowsWith(shown.rows, changesAll, bare) : null), [shown.rows, changesAll, bare]);
  // The WBS with nodes added and renamed while they wait.
  const wbsShown = useMemo(
    () => wbsWith({ order: shown.order, names: shown.names, refs: shown.refs }, changesAll),
    [shown.order, shown.names, shown.refs, changesAll],
  );
  const gridEditing = useMemo<GridEditing>(
    () => ({
      enabled: shown.summary?.writable ?? false,
      add: (activity: NewActivity) => {
        const target = wbsShown.refs?.get(activity.wbs);
        if (!target) return "the WBS couldn't be read; reload the page";
        if (target.ref === null) return `WBS ${activity.wbs} can't be named alone: another node has its code, and another its ID as a code`;
        const made = addEdit(activity, target.ref, bare);
        if (typeof made === "string") return made;
        const code = activity.code.trim();
        if (rowsShown?.code.some((other) => other.toLowerCase() === code.toLowerCase())) return `${code} is taken`;
        const duration = activity.duration.trim();
        void editing.change([
          {
            key: changeKey("add", code),
            edit: made,
            // A bare number is in the units shown.
            words: `Add ${code} ${activity.name.trim()}${duration ? `, ${duration}${/^\d+(\.\d+)?$/.test(duration) ? bare : ""}` : ""}`,
            adds: activity,
            project: target.project,
          },
        ]);
        return null;
      },
      addWbs: (parent, node) => {
        const code = node.code.trim();
        const name = node.name.trim();
        const problem = wbsProblem(wbsShown.refs, code, name, parent, null);
        if (problem) return problem;
        const above = wbsShown.refs?.get(parent);
        if (!above) return "the WBS couldn't be read; reload the page";
        if (above.ref === null) return `WBS ${parent} can't be named alone: another node has its code, and another its ID as a code`;
        const path = `${parent}.${code}`;
        // A node renamed from this code keeps its path until the rename is recorded.
        if (wbsShown.refs?.has(path)) return `${code} was another node's code until the edits waiting are applied; apply them first`;
        void editing.change([
          {
            key: changeKey("add-wbs", path),
            edit: { op: "add_wbs", code, name, parent: above.ref },
            words: `Add WBS ${code} ${name} under ${wbsShown.names?.get(parent) ?? parent}`,
            project: above.project,
            wbs: { path, code, name },
          },
        ]);
        return null;
      },
      renameWbs: (path, node) => {
        const was = wbsShown.refs?.get(path);
        if (!was) return "the WBS couldn't be read; reload the page";
        if (was.id === undefined) return "it's added once you apply the edits waiting; drop it and add it again to change it";
        if (was.projectNode) return "it's the project's own node, which follows the project";
        const code = node.code.trim();
        const name = node.name.trim();
        const parent = path.includes(".") ? path.slice(0, path.lastIndexOf(".")) : null;
        const problem = wbsProblem(wbsShown.refs, code, name, parent, path);
        if (problem) return problem;
        const newCode = code !== was.code ? code : undefined;
        const newName = name !== (wbsShown.names?.get(path) ?? "") ? name : undefined;
        if (newCode === undefined && newName === undefined) return null;
        // Renamed again while it waits: one rename, from the node as it is now.
        const key = changeKey("rename-wbs", path);
        const before = changesAll?.find((c) => c.key === key);
        const first = before?.edit.op === "rename_wbs" ? before.edit : null;
        if (!first && was.ref === null) return `WBS ${path} can't be named alone: another node has its code, and another its ID as a code`;
        // A second new code would leave the edits made since, which name the first, behind.
        const firstCode = first?.code ?? undefined;
        if (firstCode !== undefined && newCode !== undefined && newCode !== firstCode) {
          const using = (changesAll ?? []).filter((c) => c.key !== key && wbsNamed(c.edit)?.toLowerCase() === firstCode.toLowerCase());
          if (using.length > 0) return `${using.length} edit${using.length === 1 ? "" : "s"} waiting name it ${firstCode}; apply or drop them first`;
        }
        const code2 = newCode ?? firstCode;
        const name2 = newName ?? first?.name ?? undefined;
        void editing.change([
          {
            key,
            edit: {
              op: "rename_wbs",
              wbs: first?.wbs ?? was.ref ?? "",
              ...(code2 !== undefined ? { code: code2 } : {}),
              ...(name2 !== undefined ? { name: name2 } : {}),
            },
            // Named as it was: its code and name before this rename.
            words: `Rename WBS ${shown.refs?.get(path)?.code ?? was.code} ${shown.names?.get(path) ?? ""}`.trimEnd() +
              [code2 !== undefined ? ` to code ${code2}` : "", name2 !== undefined ? `${code2 !== undefined ? " and" : ""} to ${name2}` : ""].join(""),
            project: was.project,
            wbs: { path, ...(code2 !== undefined ? { code: code2 } : {}), ...(name2 !== undefined ? { name: name2 } : {}) },
            keepPlace: true,
          },
        ]);
        return null;
      },
      wbsNode: (path) => {
        const node = wbsShown.refs?.get(path);
        return node ? { code: node.code, name: wbsShown.names?.get(path) ?? "" } : undefined;
      },
      nextId: (wbs: string) => {
        const codes = rowsShown?.code ?? [];
        return nextId(
          codes.filter((_, i) => rowsShown?.wbs_path[i] === wbs),
          codes,
        );
      },
      values: edit.values,
      marks: edit.marks,
      previewed: edit.pending?.preview != null,
      commit: (typed) => void editing.commit(typed),
      apply: () => void editing.apply(),
      discard: () => editing.discard(),
      say: (text) => editing.say(text),
      isShortcut: (event) => registry.forKey(bindingOf(event)) !== undefined,
    }),
    [editing, edit.values, edit.marks, edit.pending, shown.summary, shown.names, shown.refs, wbsShown, changesAll, registry, rowsShown, bare],
  );

  startMove.current = () => {
    const rows = rowsShown;
    const ids = gridActions.current?.selected() ?? [];
    if (!rows || ids.length === 0) {
      editing.say("Select the activities to move first.");
      return;
    }
    if (!shown.summary?.writable) {
      editing.say("This schedule is read-only here.");
      return;
    }
    if (ids.some((id) => id < 0)) {
      editing.say("Activities waiting to be added move once you apply them; or drop one and add it again where it goes.");
      return;
    }
    const index = new Map(rows.id.map((id, i) => [id, i]));
    const projects = new Set(ids.map((id) => rows.project[index.get(id) ?? -1]));
    if (projects.size > 1) {
      editing.say("Move activities from one project at a time.");
      return;
    }
    const [project] = projects;
    setMoving({ codes: ids.map((id) => rows.code[index.get(id) ?? -1] ?? "").filter((code) => code !== ""), project });
  };

  startDelete.current = async () => {
    const rows = rowsShown;
    const ids = gridActions.current?.selected() ?? [];
    if (!rows || ids.length === 0) {
      editing.say("Select the activities to delete first.");
      return;
    }
    const head = shown.summary?.head;
    if (!shown.summary?.writable || !head) {
      editing.say("This schedule is read-only here.");
      return;
    }
    const index = new Map(rows.id.map((id, i) => [id, i]));
    // Activities still waiting to be added are taken out of the changes instead.
    const adds = ids.filter((id) => id < 0).map((id) => changeKey("add", rows.code[index.get(id) ?? -1] ?? ""));
    if (adds.length > 0) {
      // Taken out of the changes; not while a request is out, as they may be in it.
      if (editing.state().busy) {
        editing.say("Wait for the preview to finish, then delete again.");
        return;
      }
      void editing.drop(...adds);
    }
    const codes = ids.filter((id) => id >= 0).map((id) => rows.code[index.get(id) ?? -1] ?? "").filter((code) => code !== "");
    if (codes.length === 0) return;
    const projects = new Set(ids.filter((id) => id >= 0).map((id) => rows.project[index.get(id) ?? -1]));
    if (projects.size > 1) {
      editing.say("Delete activities from one project at a time.");
      return;
    }
    const [project] = projects;
    try {
      // A dry run counts the links and assignments that go with them.
      const { answer } = await api.edit({
        edits: codes.map((activity) => ({ op: "delete_activity" as const, activity })),
        head,
        dry_run: true,
        ...(project !== undefined ? { project } : {}),
      });
      let links = 0;
      let assignments = 0;
      for (const line of answer.applied) {
        links += Number(/with (\d+) links?/.exec(line)?.[1] ?? 0);
        assignments += Number(/(\d+) assignments?/.exec(line)?.[1] ?? 0);
      }
      setDeleting({ codes, links, assignments, ...(project !== undefined ? { project } : {}) });
    } catch (error) {
      const { message, hint } = failure(error, "The activities couldn't be deleted.");
      editing.say(message, "bad", hint);
    }
  };

  useEffect(() => {
    if (!follow) return;
    const feed = new EventFeed(api.session, {
      onEvent: (event) => {
        // The view moved: what's shown is calculated anew. A branch made or
        // dropped changes the picker.
        if (event.event === "view" || event.event === "branch") void load();
        // A proposal made or decided changes what waits for review; a head
        // that moves lists them again too (below).
        if (event.event === "proposal" && isStore.current) listProposals();
      },
      onResync: () => {
        void load();
        if (isStore.current) listProposals();
      },
      onConnection: setConnection,
      stillSignedIn: () =>
        api.summary().then(
          () => true,
          (error: unknown) => !(error instanceof ApiError && error.status === 401),
        ),
    });
    feed.start();
    return () => feed.close();
  }, [api, follow, load, listProposals]);

  // A limit set in the Usage panel waits with the other edits.
  const usageLimits = useMemo(
    () => ({ enabled: shown.summary?.writable ?? false, change: (change: Change) => void editing.change([change]) }),
    [editing, shown.summary],
  );
  const inspectorEditing = useMemo<InspectorEditing>(
    () => ({
      enabled: shown.summary?.writable ?? false,
      values: edit.values,
      marks: edit.marks,
      commit: (typed) => void editing.commit(typed),
      links: {
        enabled: shown.summary?.writable ?? false,
        // Those queued behind a request too, so a second change to a link builds on the first.
        waiting: changesWaiting(edit).filter((change) => isLinkKey(change.key)),
        marks: edit.marks,
        change: (change) => void editing.change([change]),
        drop: (key) => void editing.drop(key),
      },
    }),
    [editing, edit, shown.summary],
  );
  // A link drawn on the Gantt joins what's waiting, as one added in the inspector does.
  const writable = shown.summary?.writable ?? false;
  const shownRows = shown.rows;
  const shownLinks = shown.links;
  const onLink = useCallback(
    (link: Drawing) => {
      if (!shownRows) return;
      // Linked already: a link there that isn't waiting to be removed, or one waiting to be added.
      const waiting = changesWaiting(editing.state());
      const removing = new Set(waiting.filter((change) => change.edit.op === "remove_link").map((change) => change.key));
      const linked =
        waiting.some((change) => change.key === addKey(link.predecessor, link.successor)) ||
        (shownLinks?.predecessor.some(
          (p, k) => p === link.predecessor && shownLinks.successor[k] === link.successor && !removing.has(linkKey(shownLinks.id[k] ?? -1)),
        ) ??
          false);
      const change = addLink(shownRows, link.predecessor, link.successor, link.type, null);
      if (linked) {
        editing.say(`${change.words.replace(/ added.*/, "")} are linked already.`, "bad", "change its type or lag in the inspector");
        return;
      }
      void editing.change([change]);
    },
    [editing, shownRows, shownLinks],
  );

  useEffect(() => {
    const removers = [
      registry.register({
        id: "palette.open",
        name: "Command palette",
        label: "Search commands",
        group: "Help",
        keys: ["mod+k"],
        inPalette: false,
        run: () => setOverlay((now) => (now === "palette" ? null : "palette")),
      }),
      registry.register({
        id: "help.keys",
        name: "Keyboard shortcuts",
        group: "Help",
        keys: ["?"],
        run: () => setOverlay("keys"),
      }),
      // The views, first in the View section, each on the key its panel had.
      ...VIEWS.map(({ id, command, name, label, keys }) =>
        registry.register({
          id: command,
          name,
          label,
          group: "View",
          keys,
          // Its key again goes back to the schedule.
          run: () => setPage((now) => (now === id && id !== "schedule" ? "schedule" : id)),
        }),
      ),
      registry.register({
        id: "view.projects",
        name: "Projects: the schedule's projects, or the portfolio's to choose from",
        label: "Projects",
        group: "View",
        keys: ["alt+shift+e"],
        run: () => setPicking((now) => !now),
      }),
      registry.register({
        id: "view.panel",
        name: "Right column of the schedule",
        label: "Right column",
        group: "View",
        keys: ["mod+\\"],
        run: () => {
          if (pageNow.current !== "schedule") {
            setPage("schedule");
            setPanel(lastPanel.current);
          } else setPanel((open) => (open ? null : lastPanel.current));
        },
      }),
      registry.register({
        id: "view.changes",
        name: "Changes: edits waiting to be applied",
        label: "Changes",
        group: "Edit",
        keys: ["alt+c"],
        run: () => showPanel("changes"),
      }),
      registry.register({
        id: "view.network",
        name: "Activity network around the selection",
        label: "Network around selection",
        group: "View",
        keys: ["alt+n"],
        run: () => showPanel("network"),
      }),
      registry.register({
        id: "view.review",
        name: "Proposals to review",
        label: "Review",
        group: "Schedule",
        keys: ["alt+p"],
        run: () => showPanel("review"),
      }),
      registry.register({
        id: "view.units",
        name: "Durations and float in hours (or days)",
        label: "Hours or days",
        group: "View",
        keys: [],
        run: () => setUnits((now) => (now === "days" ? "hours" : "days")),
      }),
      registry.register({
        id: "grid.filter",
        name: "Filter activities",
        label: "Filter",
        group: "View",
        keys: ["/"],
        run: () => filter.current?.focus(),
      }),
      registry.register({
        id: "edit.undo",
        name: "Undo last change",
        label: "Undo",
        group: "Edit",
        keys: ["mod+z"],
        inText: false,
        enabled: () => editing.state().made.length > 0,
        run: () => void editing.undo(),
      }),
      registry.register({
        id: "edit.apply",
        name: "Apply previewed edits",
        label: "Apply edits",
        group: "Edit",
        keys: ["mod+enter"],
        enabled: () => editing.state().pending !== null,
        run: () => void editing.apply(),
      }),
      registry.register({
        id: "edit.discard",
        name: "Discard unapplied edits",
        label: "Discard edits",
        group: "Edit",
        keys: [],
        enabled: () => editing.state().pending !== null || editing.state().marks.size > 0,
        run: () => editing.discard(),
      }),
      registry.register({
        id: "edit.add-activity",
        name: "Add an activity in the selected activity's WBS node",
        label: "Add activity",
        group: "Edit",
        keys: ["insert"],
        enabled: () => settingsSummary.current?.writable ?? false,
        run: () => {
          setPage("schedule");
          gridActions.current?.addHere();
        },
      }),
      registry.register({
        id: "edit.add-wbs",
        name: "Add a WBS node under the selected band",
        label: "Add WBS node",
        group: "Edit",
        keys: ["alt+insert"],
        enabled: () => settingsSummary.current?.writable ?? false,
        run: () => {
          setPage("schedule");
          gridActions.current?.addWbsHere();
        },
      }),
      registry.register({
        id: "edit.move-to-wbs",
        name: "Move the selected activities to another WBS node",
        label: "Move to WBS node…",
        group: "Edit",
        keys: ["alt+m"],
        // On the schedule, where the selection is.
        enabled: () => (settingsSummary.current?.writable ?? false) && pageNow.current === "schedule",
        run: () => startMove.current(),
      }),
      registry.register({
        id: "edit.delete-activities",
        name: "Delete the selected activities",
        label: "Delete activities",
        group: "Edit",
        keys: ["delete"],
        // On the schedule, where the selection is.
        enabled: () => (settingsSummary.current?.writable ?? false) && pageNow.current === "schedule",
        run: () => void startDelete.current(),
      }),
      registry.register({
        id: "edit.preview",
        name: "Preview edits before recording",
        label: "Preview edits",
        group: "Edit",
        keys: [],
        run: () => editing.setPreviewing(!editing.state().previewing),
      }),
      registry.register({
        id: "gantt.zoom-in",
        name: "Zoom in",
        group: "View",
        keys: ["alt+="],
        run: () => setZoom((now) => stepZoom(now, fitted.current, 1)),
      }),
      registry.register({
        id: "gantt.zoom-out",
        name: "Zoom out",
        group: "View",
        keys: ["alt+-"],
        run: () => setZoom((now) => stepZoom(now, fitted.current, -1)),
      }),
      registry.register({
        id: "gantt.zoom-fit",
        name: "Zoom to fit the dates shown",
        label: "Zoom to fit",
        group: "View",
        keys: [],
        run: () => setZoom("fit"),
      }),
      ...DATE_STYLES.map(({ style, example }) =>
        registry.register({
          id: `view.dates-${style}`,
          name: `Dates as ${example}`,
          group: "View",
          keys: [],
          run: () => setDates((now) => ({ ...now, style })),
        }),
      ),
      registry.register({
        id: "view.date-times",
        name: "Times with dates (or dates alone)",
        label: "Times with dates",
        group: "View",
        keys: [],
        run: () => setDates((now) => ({ ...now, time: !now.time })),
      }),
      registry.register({
        id: "gantt.lines",
        name: "Logic lines: all, selected, none",
        label: "Logic lines",
        group: "View",
        keys: ["alt+l"],
        run: () => setLines((now) => (now === "all" ? "selected" : now === "selected" ? "none" : "all")),
      }),
      registry.register({
        id: "gantt.driving",
        name: "Driving path of the selection",
        label: "Driving path",
        group: "Schedule",
        keys: ["alt+shift+d"],
        run: () => void showPath(),
      }),
      registry.register({
        id: "explain.open",
        name: "Explain the selection's dates",
        label: "Explain",
        group: "Schedule",
        keys: ["alt+x"],
        run: () => {
          setPage("schedule");
          setPanel("inspector");
          setWhyAsked((n) => n + 1);
        },
      }),
      ...(["predecessor", "successor"] as const).map((side) =>
        registry.register({
          id: `inspector.add-${side}`,
          name: `Add a ${side} to the selected activity`,
          label: `Add ${side}`,
          group: "Edit",
          keys: [side === "predecessor" ? "alt+b" : "alt+a"],
          run: () => {
            setPage("schedule");
            setPanel("inspector");
            setAddAsked({ side, n: Date.now() });
          },
        }),
      ),
      registry.register({
        id: "inspector.focus",
        name: "Inspector: move into it, or back to the grid",
        label: "Inspector",
        group: "View",
        keys: ["alt+i"],
        run: () => {
          const at = document.activeElement;
          if (pageNow.current === "schedule" && panelNow.current === "inspector" && at instanceof Element && at.closest(".inspector")) {
            document.querySelector<HTMLElement>(".grid-body")?.focus();
            return;
          }
          setPage("schedule");
          setPanel("inspector");
          setInspectorAsked((n) => n + 1);
        },
      }),
      registry.register({
        id: "gantt.today",
        name: "Today line",
        label: "Today line",
        group: "View",
        keys: [],
        run: () => setToday((on) => !on),
      }),
      registry.register({
        id: "file.export",
        name: "Export as XER…",
        label: "Export XER",
        group: "File",
        keys: ["alt+shift+x"],
        // Only a history store can export; a plain file is the XER already.
        enabled: () => exportable.current,
        run: () => void exportXer(api, editing),
      }),
      registry.register({
        id: "file.print",
        name: "Print…",
        label: "Print",
        group: "File",
        keys: ["mod+p"],
        run: () => {
          const now = view.current();
          if (!now) {
            editing.say("There's nothing to print until the schedule has loaded.");
            return;
          }
          const columns = (gridNow.current?.columns ?? COLUMNS.map((c) => ({ field: c.id, width: c.width }))).filter(
            (c) => !COMPARE_COLUMNS.has(c.field),
          );
          setPrinting({
            rows: now.rows,
            // Add activity lines aren't printed.
            lines: now.arranged.lines.filter((value) => {
              const band = value < 0 ? now.arranged.bands[-value - 1] : undefined;
              return !band || (band.adds === undefined && band.newWbs === undefined);
            }),
            bands: now.arranged.bands,
            columns: columns.map((c) => ({ id: c.field, width: c.width })),
            // As it was too, so the title block names what's printed.
            summary: settingsSummary.current,
          });
        },
      }),
      registry.register({
        id: "layout.save",
        name: "Save layout",
        group: "View",
        keys: [],
        run: () => {
          const name = settings.current.layoutName;
          if (name) {
            void saveLayout(name).catch((error: unknown) => {
              const { message, hint } = failure(error, "The layout couldn't be saved.");
              editing.say(message, "bad", hint);
            });
          } else {
            setLayoutMode("save");
            setOverlay("layouts");
          }
        },
      }),
      registry.register({
        id: "layout.save-as",
        name: "Save layout as…",
        group: "View",
        keys: [],
        run: () => {
          setLayoutMode("save");
          setOverlay("layouts");
        },
      }),
      registry.register({
        id: "layout.open",
        name: "Open layout…",
        group: "View",
        keys: [],
        run: () => {
          setLayoutMode("open");
          setOverlay("layouts");
        },
      }),
      registry.register({
        id: "layout.delete",
        name: "Delete layout…",
        group: "View",
        keys: [],
        run: () => {
          setLayoutMode("delete");
          setOverlay("layouts");
        },
      }),
      registry.register({
        id: "data.reload",
        name: "Reload schedule",
        label: "Reload",
        group: "File",
        keys: ["alt+r"],
        run: () =>
          void load().then((loaded) =>
            editing.say(loaded ? "Reloaded the schedule." : "The schedule couldn't be reloaded.", loaded ? "done" : "bad"),
          ),
      }),
    ];
    return () => removers.forEach((remove) => remove());
  }, [registry, load, editing, showPath, api, saveLayout, view, showPanel]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // The merge dialog takes every key; another open dialog takes its own,
      // but the palette's key still toggles it.
      if (merging || printing || deleting) return;
      if (overlay && registry.forKey(bindingOf(event))?.id !== "palette.open") return;
      if (!appTakes(registry, event, event.target)) return;
      if (registry.press(event)) event.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [registry, overlay, merging, printing, deleting]);

  const close = () => setOverlay(null);
  const waiting = Array.isArray(proposals) ? proposals.length : 0;
  // Why the proposals couldn't be listed, so the badge doesn't say none wait.
  const unlisted = proposals && !Array.isArray(proposals) ? proposals.message : null;
  const outlook: Outlook = useMemo(
    () => outlookOf(shown.rows, shown.summary, edit.pending?.preview ?? null, edit.pending?.batch.edits),
    // Floats are written in the units shown.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [shown.rows, shown.summary, edit.pending, units],
  );

  return (
    <div className="shell">
      <TitleBlock
        summary={shown.summary}
        connection={connection}
        mac={mac}
        onSearch={() => setOverlay("palette")}
        project={
          <ProjectPicker
            summary={shown.summary}
            open={picking}
            onOpen={setPicking}
            portfolio={
              shown.summary?.portfolio ? (
                <ProjectsPanel
                  api={api}
                  shown={shown.summary.location}
                  onOpened={async () => {
                    setPicking(false);
                    await loadShown();
                  }}
                  onClose={() => setPicking(false)}
                />
              ) : undefined
            }
          />
        }
        views={VIEWS.map(({ id, label, keys }) => ({ id, label, key: keys[0] }))}
        view={page}
        onView={setPage}
        branch={
          shown.summary?.head ? (
            <BranchPicker
              api={api}
              summary={shown.summary}
              onSwitch={switchTo}
              onChanged={loadShown}
              say={say}
              onMerge={setMerging}
            />
          ) : undefined
        }
        outlook={outlook}
        review={
          shown.summary?.head ? (
            <button
              type="button"
              className="title-button review-badge"
              aria-label={unlisted ? "Proposals to review: couldn't be listed" : `Proposals to review: ${waiting}`}
              title={unlisted ?? "Proposals to review"}
              onClick={() => {
                setPage("schedule");
                setPanel("review");
              }}
            >
              <Icon name="review" />
              <span className="title-words">Review</span> <span className={`count${waiting > 0 ? " waiting" : ""}`}>{unlisted ? "!" : waiting}</span>
            </button>
          ) : undefined
        }
        menu={<MenuButton registry={registry} mac={mac} />}
      />
      <Toolbar
        registry={registry}
        mac={mac}
        layout={layoutName}
        zoom={zoom}
        onZoom={setZoom}
        toggles={[
          // The toolbar turns lines off and back on; Alt+L steps through all, selected and none.
          { command: "gantt.lines", icon: "link", pressed: lines !== "none", onToggle: () => setLines((now) => (now === "none" ? "all" : "none")) },
          { command: "gantt.driving", icon: "drive", pressed: path !== null },
          { command: "view.panel", icon: "panel", pressed: panel !== null && page === "schedule" },
        ]}
        onSlot={setToolbar}
        onAdd={
          shown.summary?.writable
            ? () => {
                setPage("schedule");
                gridActions.current?.addHere();
              }
            : undefined
        }
      />
      <div className="workspace" id="view-schedule" role="tabpanel" aria-labelledby="view-tab-schedule" inert={page !== "schedule"}>
        <main className="views" style={{ gridTemplateColumns: `minmax(0, ${split}fr) 5px minmax(0, ${1 - split}fr)` }}>
          {failed ? (
            <section role="alert" className="failed">
              <p>{failed.message}</p>
              {failed.hint && <p className="muted">{failed.hint}</p>}
            </section>
          ) : (
            <>
              <section className="pane grid-pane" aria-label="Activity grid">
                {edit.banner && (
                  <div className="banner" role="alert">
                    <span>{edit.banner}</span>
                    {edit.pending && (
                      <button type="button" onClick={() => void editing.apply()} disabled={edit.busy}>
                        Apply again
                      </button>
                    )}
                    <button type="button" onClick={() => (edit.pending ? editing.discard() : editing.dismissBanner())} disabled={edit.busy}>
                      {edit.pending ? "Discard" : "Dismiss"}
                    </button>
                  </div>
                )}
                <Grid
                  rows={rowsShown}
                  actions={gridActions}
                  filterRef={filter}
                  editing={gridEditing}
                  view={view}
                  path={path?.ids ?? null}
                  bandOrder={wbsShown.order ?? undefined}
                  bandNames={wbsShown.names ?? undefined}
                  units={units}
                  dates={dates}
                  compared={compared}
                  layout={gridLayout}
                  onLayout={onGridLayout}
                  finder={finder}
                  toolbar={toolbar}
                  status={statusSlot}
                />
              </section>
              <Splitter split={split} onSplit={setSplit} onDone={keepSplit} />
              <section className="pane gantt-pane" aria-label="Gantt chart">
                <Gantt
                  view={view}
                  summary={shown.summary}
                  zoom={zoom}
                  onZoom={setZoom}
                  onFitted={onFitted}
                  today={today}
                  links={shown.links}
                  lines={lines}
                  path={litOf(path, highlight, chain)}
                  onLink={writable ? onLink : undefined}
                />
              </section>
            </>
          )}
        </main>
        {panel && (
          <aside className={`right-column${panel === "inspector" ? " flush" : ""}`} aria-label={PANEL_NAMES[panel]}>
            {panel === "changes" ? (
              <ChangesPanel editing={editing} state={edit} rows={shown.rows} view={view} mac={mac} onClose={() => setPanel(null)} />
            ) : panel === "inspector" ? (
              <Inspector
                api={api}
                view={view}
                links={shown.links}
                fields={shown.fields}
                summary={shown.summary}
                edit={inspectorEditing}
                dates={dates}
                onClose={() => setPanel(null)}
                onChain={setChain}
              />
            ) : panel === "network" ? (
              <NetworkPanel view={view} links={shown.links} onClose={() => setPanel(null)} />
            ) : (
              <ReviewPanel
                api={api}
                summary={shown.summary}
                rows={shown.rows}
                view={view}
                proposals={proposals}
                onClose={() => setPanel(null)}
                onChanged={async () => {
                  await loadShown();
                  listProposals();
                }}
                say={say}
                onCompared={setCompared}
                onHighlight={setHighlight}
              />
            )}
          </aside>
        )}
      </div>
      {page === "network" && (
        <section className="view-page pane view-network" id="view-network" role="tabpanel" tabIndex={-1} aria-label="Activity network">
          <ProjectNetwork
            view={view}
            links={shown.links}
            onNeighbourhood={() => {
              setPage("schedule");
              setPanel("network");
            }}
          />
        </section>
      )}
      {page === "resources" && (
        <div className="view-page" id="view-resources" role="tabpanel" tabIndex={-1} aria-labelledby="view-tab-resources">
          <UsagePanel
            api={api}
            resources={shown.summary?.inspect.resources ?? []}
            head={scheduleIdentity(shown.summary)}
            onClose={() => setPage("schedule")}
            limits={usageLimits}
          />
        </div>
      )}
      {page === "risk" && (
        <div className="view-page" id="view-risk" role="tabpanel" tabIndex={-1} aria-labelledby="view-tab-risk">
          <RiskPanel
            api={api}
            view={view}
            projects={shown.summary?.inspect.projects ?? []}
            head={scheduleIdentity(shown.summary)}
            memory={risk.current}
            onClose={() => setPage("schedule")}
            onShow={() => setPage("schedule")}
          />
        </div>
      )}
      {(page === "history" || historyKept) && (
        <div className="view-page" id="view-history" role="tabpanel" tabIndex={-1} aria-labelledby="view-tab-history" hidden={page !== "history"}>
          <HistoryPanel
            api={api}
            summary={shown.summary}
            rows={shown.rows}
            view={view}
            onClose={() => setPage("schedule")}
            onChanged={loadShown}
            say={say}
            onCompared={setCompared}
            proposed={proposals}
            active={page === "history"}
          />
        </div>
      )}
      <StatusBar onSlot={setStatusSlot}>
        <EditBar
          editing={editing}
          state={edit}
          moved={outlook.pending?.moved ?? null}
          calculated={shown.summary?.calculated ? calculatedAt : null}
          mac={mac}
          saved={savedWords(shown.summary)}
          onOpen={() => {
            setPage("schedule");
            setPanel("changes");
          }}
        />
      </StatusBar>
      {printing && (
        <PrintView
          rows={printing.rows}
          lines={printing.lines}
          bands={printing.bands}
          columns={printing.columns}
          split={split}
          summary={printing.summary}
          onClose={closePrint}
        />
      )}
      {deleting && (
        <DeleteDialog
          deleting={deleting}
          onClose={() => {
            setDeleting(null);
            document.querySelector<HTMLElement>(".grid-body")?.focus();
          }}
          onDelete={() => {
            const names = new Map((rowsShown?.code ?? []).map((code, i) => [code, rowsShown?.name[i] ?? ""]));
            void editing.change(
              deleting.codes.map((activity) => ({
                key: changeKey("delete", activity),
                edit: { op: "delete_activity" as const, activity },
                words: `Delete ${activity} ${names.get(activity) ?? ""}`.trim(),
                ...(deleting.project !== undefined ? { project: deleting.project } : {}),
              })),
            );
            setDeleting(null);
            document.querySelector<HTMLElement>(".grid-body")?.focus();
          }}
        />
      )}
      {moving && wbsShown.order && (
        <WbsPicker
          title={`Move ${moving.codes.length === 1 ? moving.codes[0] : `${moving.codes.length} activities`} to which WBS node?`}
          choices={wbsChoices(wbsShown.order, wbsShown.names).filter(
            (choice) => moving.project === undefined || wbsShown.refs?.get(choice.path)?.project === moving.project,
          )}
          onClose={() => {
            setMoving(null);
            document.querySelector<HTMLElement>(".grid-body")?.focus();
          }}
          onPick={(path) => {
            const target = wbsShown.refs?.get(path);
            // Those already there, as moves waiting show them, stay out.
            const where = new Map((rowsShown?.code ?? []).map((code, i) => [code, rowsShown?.wbs_path[i]]));
            const codes = moving.codes.filter((code) => where.get(code) !== path);
            if (codes.length === 0) {
              editing.say(moving.codes.length === 1 ? `${moving.codes[0] ?? ""} is in that node already.` : "They're in that node already.");
            } else if (!target || target.ref === null) {
              editing.say(`WBS ${path} can't be named alone: another node has its code, and another its ID as a code.`, "bad");
            } else {
              const named = wbsShown.names?.get(path);
              void editing.change(
                codes.map((activity) => ({
                  key: changeKey("move", activity),
                  edit: { op: "set_wbs" as const, activity, wbs: target.ref ?? "" },
                  words: `Move ${activity} to WBS ${named ? `${named} (${path})` : path}`,
                  project: target.project,
                  moves: path,
                })),
              );
            }
            setMoving(null);
            document.querySelector<HTMLElement>(".grid-body")?.focus();
          }}
        />
      )}
      {merging && shown.summary && (
        <MergeDialog
          api={api}
          summary={shown.summary}
          rows={shown.rows}
          view={view}
          from={merging}
          onClose={() => setMerging(null)}
          onChanged={loadShown}
          say={say}
          onCompared={setCompared}
        />
      )}
      {overlay === "palette" && <Palette registry={registry} mac={mac} onClose={close} />}
      {overlay === "keys" && <KeyList registry={registry} mac={mac} onClose={close} />}
      {overlay === "layouts" && (
        <LayoutDialog
          api={api}
          mode={layoutMode}
          current={layoutName}
          onSave={async (name) => {
            await saveLayout(name);
            close();
          }}
          onOpen={async (name) => {
            await openLayout(name);
            close();
          }}
          onDeleted={(name) => {
            // A restore still on its way mustn't bring it back.
            layoutChosen.current++;
            const same = (other: string | null) => other !== null && other.toLowerCase() === name.toLowerCase();
            if (same(layoutName)) setLayoutName(null);
            // Remembered, if it was, even if it never opened.
            let remembered: string | null = null;
            try {
              remembered = localStorage.getItem(layoutKey(settings.current.location));
            } catch {
              // No storage: nothing remembered.
            }
            if (same(layoutName) || same(remembered)) rememberLayout(null);
            editing.say(`Deleted layout ${name}.`, "done");
          }}
          onClose={close}
        />
      )}
    </div>
  );
}
