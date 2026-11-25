// One activity's resources, codes, user fields and notes, from
// `GET /api/1/query`: its text filter on the activity's ID, with a column
// for each code type and user field, then the row whose `task_id` is the
// activity's.

import type { Api, Fields, QueryParams } from "../api";

/** What the inspector's collapsed sections show. */
export interface Details {
  /** Assigned resources, by short name. */
  resources: string[];
  /** Assigned roles, by short name. */
  roles: string[];
  /** Activity code values held, by code type. */
  codes: { type: string; value: string }[];
  /** User field values set, by label; a date as the server writes it, to show in the format chosen. */
  udfs: { label: string; value: string; date: boolean }[];
  /** Notebook notes, by topic. */
  notes: { topic: string; text: string }[];
}

/** A column name in a list `query` splits on commas: `\,` for a comma in it and `\\` for a backslash. */
export function listItem(name: string): string {
  return name.replace(/\\/g, "\\\\").replace(/,/g, "\\,");
}

/**
 * Whether `query` can take a column named `name`: it trims each name in its
 * list, so one with a space at either end, or none at all, would be refused,
 * and the whole query with it.
 */
function askable(name: string): boolean {
  return name !== "" && name === name.trim();
}

/** The code types by name, once each; project code types can share one. */
function codeTypes(fields: Fields | null): string[] {
  return [...new Set((fields?.activity_codes ?? []).map((type) => type.name))].filter(askable);
}

/** The user fields by label, once each, and whether each holds a date. */
function udfs(fields: Fields | null): { label: string; date: boolean }[] {
  const seen = new Map<string, boolean>();
  for (const udf of fields?.activity_udfs ?? []) {
    if (askable(udf.label) && !seen.has(udf.label)) seen.set(udf.label, udf.data_type.endsWith("date"));
  }
  return [...seen].map(([label, date]) => ({ label, date }));
}

/**
 * The query that finds activity `code` with every detail column. It isn't
 * narrowed to the activity's project: the fields' code types are the whole
 * file's, and a project without one of them refuses its column.
 */
export function detailsQuery(code: string, fields: Fields | null): QueryParams {
  const columns = [
    "id",
    "resources",
    "roles",
    "notes",
    // Without the list of code types, the codes come joined in one column.
    ...(fields ? codeTypes(fields).map((name) => `code:${name}`) : ["codes"]),
    ...udfs(fields).map((udf) => `udf:${udf.label}`),
  ];
  return { text: code, columns: columns.map(listItem).join(",") };
}

/** The text of a cell, or empty for none. */
function textOf(value: unknown): string {
  if (value === null || value === undefined) return "";
  return typeof value === "string" ? value : JSON.stringify(value);
}

/** A list `query` joins with commas, such as the short names of resources. */
function listOf(value: unknown): string[] {
  return textOf(value)
    .split(", ")
    .map((item) => item.trim())
    .filter(Boolean);
}

/**
 * The details in `row`, a row of `detailsQuery`'s answer. Notes come one
 * topic each, separated by a blank line: a note's own text never holds one.
 */
export function detailsOf(row: Record<string, unknown>, fields: Fields | null): Details {
  const codes = fields
    ? codeTypes(fields).flatMap((type) => {
        const value = textOf(row[`code:${type}`]);
        return value ? [{ type, value }] : [];
      })
    : listOf(row.codes).map((held) => {
        const at = held.indexOf("=");
        return at < 0 ? { type: "", value: held } : { type: held.slice(0, at), value: held.slice(at + 1) };
      });
  const set = udfs(fields).flatMap((udf) => {
    const value = textOf(row[`udf:${udf.label}`]);
    return value ? [{ label: udf.label, value, date: udf.date }] : [];
  });
  const notes = textOf(row.notes)
    .split("\n\n")
    .filter((note) => note.trim() !== "")
    .map((note) => {
      const at = note.indexOf(": ");
      return at < 0 ? { topic: "", text: note } : { topic: note.slice(0, at), text: note.slice(at + 2) };
    });
  return { resources: listOf(row.resources), roles: listOf(row.roles), codes, udfs: set, notes };
}

/** Activity `id`'s details, or null if the query doesn't find it. */
export async function fetchDetails(
  api: Api,
  of: { id: number; code: string },
  fields: Fields | null,
): Promise<Details | null> {
  const answer = await api.query(detailsQuery(of.code, fields));
  const row = answer.rows.find((r) => (r as Record<string, unknown>).id === of.id) as Record<string, unknown> | undefined;
  return row ? detailsOf(row, fields) : null;
}
