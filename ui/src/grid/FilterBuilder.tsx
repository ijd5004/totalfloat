// The grid's filter builder: conditions from `query`'s filters, which the
// server applies when they're applied, so the grid finds what `totalfloat
// query` would.

import { useCallback, useEffect, useId, useRef, useState } from "react";

import { failure } from "../api";

import {
  type Built,
  CHOICE_BY_ID,
  type Condition,
  type FilterField,
  STATUS_VALUES,
  TYPE_VALUES,
  builtOf,
  codeParts,
  complete,
  conditionsOf,
  groupsIn,
  unused,
} from "./filters";

/** Filters kept by name beside the schedule, each as `query`'s parameters and `any` groups. */
export interface SavedFilters {
  list(): Promise<string[]>;
  read(name: string): Promise<{
    built?: Readonly<Record<string, string | undefined>>;
    any?: readonly Readonly<Record<string, string | undefined>>[];
  }>;
  save(name: string, filter: Built): Promise<void>;
  remove(name: string): Promise<void>;
}

/** What an error from the server says, for the builder to show: its message, then its hint as a sentence of its own. */
export function said(error: unknown): string {
  const { message, hint } = failure(error, error instanceof Error ? error.message : String(error));
  if (!hint) return message;
  const stop = /[.!?]$/.test(message) ? "" : ".";
  return `${message}${stop} ${hint.charAt(0).toUpperCase()}${hint.slice(1)}`;
}

/** An activity code type and its values, which a code condition picks from. */
export interface CodeChoices {
  type: string;
  values: string[];
}

export function FilterBuilder({
  applied,
  codes,
  saved,
  onApply,
  onClear,
  onClose,
}: {
  /** The conditions applied now, which the builder starts from. */
  applied: readonly Condition[];
  codes: readonly CodeChoices[];
  /** Named filters, where there's somewhere to keep them. */
  saved?: SavedFilters | undefined;
  /** Applies the conditions; resolves to why they couldn't be, or null. */
  onApply: (conditions: Condition[]) => Promise<string | null>;
  onClear: () => void;
  onClose: () => void;
}) {
  const [conditions, setConditions] = useState<Condition[]>(() => [...applied]);
  // The "any of" groups shown, including any not given a condition yet.
  const [groups, setGroups] = useState<number[]>(() => groupsIn(applied));
  const [problem, setProblem] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const change = (at: number, value: string) =>
    setConditions((now) => now.map((c, k) => (k === at ? { ...c, value } : c)));
  const add = (field: FilterField, group?: number) => {
    const first = CHOICE_BY_ID.get(field)?.input;
    const value =
      first === "status"
        ? STATUS_VALUES[0]
        : first === "type"
          ? TYPE_VALUES[0]
          : first === "code"
            ? `${codes[0]?.type ?? ""}=${codes[0]?.values[0] ?? ""}`
            : "";
    setConditions((now) => [...now, group === undefined ? { field, value } : { field, value, group }]);
    setProblem(null);
  };
  const removeGroup = (group: number) => {
    setConditions((now) => now.filter((c) => c.group !== group));
    setGroups((now) => now.filter((g) => g !== group));
  };

  // Bumped by each apply, Clear and closing, so a saved filter read before
  // one of them isn't applied after it.
  const generation = useRef(0);
  useEffect(
    () => () => {
      generation.current += 1;
    },
    [],
  );

  const apply = async (these: Condition[] = conditions, shown: number[] = groups) => {
    generation.current += 1;
    setNotice(null);
    const usable = these.filter(complete);
    if (usable.length < these.length) {
      setProblem("Give each condition a value, or remove it.");
      return;
    }
    if (shown.some((group) => !these.some((c) => c.group === group))) {
      setProblem("Give each “any of” group a condition, or remove the group.");
      return;
    }
    setBusy(true);
    const refused = await onApply(usable);
    setBusy(false);
    setProblem(refused);
  };

  // The named filters, listed when the builder opens and after each change.
  const [names, setNames] = useState<string[]>([]);
  const [chosen, setChosen] = useState("");
  const [naming, setNaming] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [listing, setListing] = useState(0);
  const relist = useCallback(() => setListing((n) => n + 1), []);
  useEffect(() => {
    if (!saved) return;
    let current = true;
    saved.list().then(
      (listed) => {
        if (!current) return;
        setNames(listed);
        setChosen((now) => (listed.includes(now) ? now : (listed[0] ?? "")));
      },
      (error: unknown) => {
        if (current) setProblem(`The saved filters couldn't be listed: ${said(error)}`);
      },
    );
    return () => {
      current = false;
    };
  }, [saved, listing]);

  const applySaved = async () => {
    if (!saved || !chosen) return;
    setNotice(null);
    const mine = ++generation.current;
    let read: Condition[];
    try {
      const filter = await saved.read(chosen);
      read = conditionsOf(filter.built ?? {}, filter.any ?? []);
    } catch (error) {
      if (generation.current === mine) setProblem(`${chosen} couldn't be read: ${said(error)}`);
      return;
    }
    // Cleared, closed or applied since: this one's answer is too late.
    if (generation.current !== mine) return;
    const shown = groupsIn(read);
    setConditions(read);
    setGroups(shown);
    await apply(read, shown);
  };

  const saveAs = async () => {
    const name = naming.trim();
    if (!saved || name === "" || busy) return;
    setNotice(null);
    const usable = conditions.filter(complete);
    if (usable.length === 0 || usable.length < conditions.length) {
      setProblem("Give each condition a value before saving, and at least one condition.");
      return;
    }
    if (groups.some((group) => !usable.some((c) => c.group === group))) {
      setProblem("Give each “any of” group a condition before saving, or remove the group.");
      return;
    }
    setBusy(true);
    try {
      await saved.save(name, builtOf(usable));
    } catch (error) {
      setProblem(`The filter couldn't be saved: ${said(error)}`);
      return;
    } finally {
      setBusy(false);
    }
    setProblem(null);
    setNotice(`Saved filter ${name}.`);
    setNaming("");
    setChosen(name);
    relist();
  };

  const removeSaved = async () => {
    if (!saved || !chosen || busy) return;
    setConfirming(false);
    setNotice(null);
    setBusy(true);
    try {
      await saved.remove(chosen);
    } catch (error) {
      setProblem(`${chosen} couldn't be deleted: ${said(error)}`);
      return;
    } finally {
      setBusy(false);
    }
    setProblem(null);
    setNotice(`Deleted filter ${chosen}.`);
    relist();
  };

  return (
    <div
      className="filter-builder"
      role="group"
      aria-label="Filter builder"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onClose();
        }
      }}
    >
      {saved && (
        <div className="filter-saved">
          {names.length > 0 ? (
            <>
              <select
                aria-label="Saved filter"
                value={chosen}
                onChange={(event) => {
                  setChosen(event.target.value);
                  setConfirming(false);
                }}
              >
                {names.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
              <button type="button" disabled={busy} onClick={() => void applySaved()}>
                Apply saved
              </button>
              {confirming ? (
                <>
                  <span>Delete {chosen}?</span>
                  <button type="button" disabled={busy} aria-label={`Delete saved filter ${chosen}`} onClick={() => void removeSaved()}>
                    Delete
                  </button>
                  <button type="button" onClick={() => setConfirming(false)}>
                    Keep
                  </button>
                </>
              ) : (
                <button type="button" disabled={busy} onClick={() => setConfirming(true)}>
                  Delete…
                </button>
              )}
            </>
          ) : (
            <span className="muted">No saved filters yet.</span>
          )}
          <input
            aria-label="Filter name"
            placeholder="Name to save as"
            value={naming}
            onChange={(event) => setNaming(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void saveAs();
            }}
          />
          <button type="button" disabled={busy || naming.trim() === ""} onClick={() => void saveAs()}>
            Save as
          </button>
        </div>
      )}
      {conditions.length === 0 && groups.length === 0 && (
        <p className="muted">Add conditions; an activity has to meet them all.</p>
      )}
      <ConditionList
        conditions={conditions}
        codes={codes}
        onChange={change}
        onRemove={(at) => setConditions((now) => now.filter((_, k) => k !== at))}
        onAdd={(field) => add(field)}
      />
      {groups.map((group, k) => (
        <fieldset className="filter-group" key={group} aria-label={`Any of, group ${k + 1}`}>
          <legend>
            {k === 0 ? "And any of these groups — group 1" : `or group ${k + 1}`}
            <button type="button" aria-label={`Remove group ${k + 1}`} onClick={() => removeGroup(group)}>
              ×
            </button>
          </legend>
          <ConditionList
            conditions={conditions}
            group={group}
            groupName={`group ${k + 1}`}
            codes={codes}
            onChange={change}
            onRemove={(at) => setConditions((now) => now.filter((_, j) => j !== at))}
            onAdd={(field) => add(field, group)}
          />
        </fieldset>
      ))}
      <div className="filter-actions">
        <button
          type="button"
          onClick={() => {
            setGroups((now) => [...now, (now.at(-1) ?? -1) + 1]);
            setProblem(null);
          }}
        >
          Add an “any of” group
        </button>
        <button type="button" disabled={busy} onClick={() => void apply()}>
          Apply
        </button>
        <button
          type="button"
          onClick={() => {
            generation.current += 1;
            setConditions([]);
            setGroups([]);
            setProblem(null);
            setNotice(null);
            onClear();
          }}
        >
          Clear
        </button>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </div>
      {problem && (
        <p className="filter-problem" role="alert">
          {problem}
        </p>
      )}
      {notice && !problem && (
        <p className="muted" role="status">
          {notice}
        </p>
      )}
    </div>
  );
}

/** One list of conditions: the ones that must all hold, or one "any of" group's. */
function ConditionList({
  conditions,
  group,
  groupName,
  codes,
  onChange,
  onRemove,
  onAdd,
}: {
  conditions: readonly Condition[];
  group?: number;
  /** How the group's controls are named, as "group 2"; none for the main list. */
  groupName?: string;
  codes: readonly CodeChoices[];
  onChange: (at: number, value: string) => void;
  onRemove: (at: number) => void;
  onAdd: (field: FilterField) => void;
}) {
  const offered = unused(conditions, group).filter((c) => c.id !== "code" || codes.length > 0);
  const named = (label: string) => (groupName ? `${label}, ${groupName}` : label);
  return (
    <>
      {conditions.map((condition, at) => {
        if (condition.group !== group) return null;
        const choice = CHOICE_BY_ID.get(condition.field);
        const label = named(choice?.label ?? condition.field);
        return (
          <div className="filter-condition" key={`${condition.group ?? "all"}-${condition.field}`}>
            <span className="filter-field">{choice?.label ?? condition.field}</span>
            <ValueControl condition={condition} label={label} codes={codes} onChange={(value) => onChange(at, value)} />
            <button type="button" aria-label={`Remove ${label}`} onClick={() => onRemove(at)}>
              ×
            </button>
          </div>
        );
      })}
      {offered.length > 0 && (
        <select
          aria-label={named("Add a condition")}
          value=""
          onChange={(event) => {
            const field = event.target.value as FilterField;
            if (field) onAdd(field);
          }}
        >
          <option value="">Add a condition…</option>
          {offered.map((c) => (
            <option key={c.id} value={c.id}>
              {c.label}
            </option>
          ))}
        </select>
      )}
    </>
  );
}

function ValueControl({
  condition,
  label,
  codes,
  onChange,
}: {
  condition: Condition;
  label: string;
  codes: readonly CodeChoices[];
  onChange: (value: string) => void;
}) {
  const id = useId();
  const input = CHOICE_BY_ID.get(condition.field)?.input;
  const named = `${label} value`;
  switch (input) {
    case "none":
      return null;
    case "code": {
      // The type picked from the file's, and its value typed, offered the
      // values the summary lists: a code type can have more than it lists.
      const { type, value } = codeParts(condition.value);
      const known = codes.find((code) => code.type === type)?.values ?? [];
      const list = `filter-code-values-${id}`;
      return (
        <>
          <select
            aria-label={`${label} type`}
            value={type}
            onChange={(e) => onChange(`${e.target.value}=${value}`)}
          >
            {codes.map((code) => (
              <option key={code.type} value={code.type}>
                {code.type}
              </option>
            ))}
          </select>
          <input
            aria-label={named}
            list={list}
            value={value}
            onChange={(e) => onChange(`${type}=${e.target.value}`)}
          />
          <datalist id={list}>
            {known.map((v) => (
              <option key={v} value={v} />
            ))}
          </datalist>
        </>
      );
    }
    case "status":
    case "type":
      return (
        <select aria-label={named} value={condition.value} onChange={(e) => onChange(e.target.value)}>
          {(input === "status" ? STATUS_VALUES : TYPE_VALUES).map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      );
    default:
      return (
        <input
          aria-label={named}
          type={input === "date" ? "date" : input === "number" ? "number" : "text"}
          value={condition.value}
          onChange={(e) => onChange(e.target.value)}
        />
      );
  }
}
