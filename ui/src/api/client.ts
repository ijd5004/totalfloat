// The client for `totalfloat serve`'s JSON API under /api/1/: one method per
// endpoint, with the API's own types.

import type {
  BaselineChange,
  CapacityReport,
  Baselines,
  BranchChange,
  Branches,
  ChangesetDocument,
  CheckReport,
  EditReport,
  EditRequest,
  ErrorDocument,
  Explanation,
  Fields,
  HistoryDiff,
  HistoryLog,
  Layout,
  Layouts,
  Links,
  Merged,
  NamedFilter,
  NamedFilters,
  PortfolioReport,
  Problem,
  Proposals,
  QueryParams,
  QueryReport,
  Reverted,
  RiskInputs,
  RiskReport,
  Reviewed,
  Rows,
  Summary,
  UsageReport,
  Wbs,
} from "./types";

export const SESSION_HEADER = "x-totalfloat-session";
/** The performance mark set when rows have arrived and been read. */
export const ROWS_ARRIVED = "totalfloat:rows-arrived";
/** The header that carries the head shown after a change. */
export const HEAD_HEADER = "x-totalfloat-head";

/** A refusal, as `totalfloat.error/1`, with every problem it lists. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly hint: string;
  readonly problems: Problem[];

  constructor(status: number, code: string, message: string, hint: string, problems: Problem[] = []) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.hint = hint;
    this.problems = problems;
  }
}

function isErrorDocument(body: unknown): body is ErrorDocument {
  return (
    typeof body === "object" &&
    body !== null &&
    (body as { schema?: unknown }).schema === "totalfloat.error/1" &&
    typeof (body as { error?: unknown }).error === "object"
  );
}

function isEditReport(body: unknown): body is EditReport {
  return (
    typeof body === "object" &&
    body !== null &&
    (body as { schema?: unknown }).schema === "totalfloat.edit/1" &&
    Array.isArray((body as { projects?: unknown }).projects)
  );
}

/** Reads a refusal's `totalfloat.error/1` body, or describes a bare status. */
export async function refusal(response: Response): Promise<ApiError> {
  try {
    const body: unknown = await response.json();
    if (isErrorDocument(body)) {
      const { code, message, hint, problems } = body.error;
      return new ApiError(response.status, code, message, hint, problems ?? []);
    }
  } catch {
    // Not JSON; fall through.
  }
  return new ApiError(response.status, "http-error", `The server answered ${response.status}.`, "");
}

/** A failure's words: a refusal's own message and hint, or `otherwise` for anything else. */
export function failure(error: unknown, otherwise: string): { message: string; hint: string } {
  return error instanceof ApiError ? { message: error.message, hint: error.hint } : { message: otherwise, hint: "" };
}

/** What a change answers: the command's JSON, and the head now shown. */
export interface Changed<T> {
  answer: T;
  head: string | null;
}

/** A point in the store's history: a branch, baseline or changeset. */
export interface At {
  at?: string;
}

type Query = Record<string, string | number | boolean | undefined>;

function withQuery(path: string, query: Query = {}): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined) params.set(key, String(value));
  }
  const text = params.toString();
  return text ? `${path}?${text}` : path;
}

/** A client bound to one session. */
export class Api {
  readonly session: string | null;
  private readonly fetcher: typeof fetch;

  constructor(session: string | null, fetcher: typeof fetch = fetch.bind(globalThis)) {
    this.session = session;
    this.fetcher = fetcher;
  }

  private headers(): Record<string, string> {
    const headers: Record<string, string> = { accept: "application/json" };
    if (this.session) headers[SESSION_HEADER] = this.session;
    return headers;
  }

  private async send(path: string, init: RequestInit): Promise<Response> {
    const response = await this.fetcher(path, { ...init, credentials: "same-origin" });
    if (!response.ok) throw await refusal(response);
    return response;
  }

  private static async json<T>(response: Response, path: string): Promise<T> {
    try {
      return (await response.json()) as T;
    } catch {
      throw new ApiError(
        response.status,
        "bad-answer",
        `The server's answer to ${path} wasn't JSON.`,
        "Reload the page; if it persists, the app and the server may be from different builds.",
      );
    }
  }

  private async get<T>(path: string, query?: Query): Promise<T> {
    const url = withQuery(path, query);
    return Api.json<T>(await this.send(url, { headers: this.headers() }), url);
  }

  /**
   * Posts `body`. A 422 whose body `answers` accepts is given as the answer:
   * a dry run a newly failing check blocks still reports what it did.
   */
  private async post<T>(path: string, body: object, answers?: (body: unknown) => body is T): Promise<Changed<T>> {
    const init: RequestInit = {
      method: "POST",
      headers: { ...this.headers(), "content-type": "application/json" },
      body: JSON.stringify(body),
      credentials: "same-origin",
    };
    const response = await this.fetcher(path, init);
    if (!response.ok) {
      if (answers && response.status === 422) {
        const answer: unknown = await response
          .clone()
          .json()
          .catch(() => null);
        if (answers(answer)) return { answer, head: response.headers.get(HEAD_HEADER) };
      }
      throw await refusal(response);
    }
    return { answer: await Api.json<T>(response, path), head: response.headers.get(HEAD_HEADER) };
  }

  // ---- What's shown ----
  summary(): Promise<Summary> {
    return this.get("/api/1/summary");
  }
  async rows(point: At = {}): Promise<Rows> {
    const rows = await this.get<Rows>("/api/1/rows", { ...point });
    // For timing how long the grid takes to paint once the rows are here.
    performance.clearMarks(ROWS_ARRIVED);
    performance.mark(ROWS_ARRIVED);
    return rows;
  }
  links(point: At = {}): Promise<Links> {
    return this.get("/api/1/links", { ...point });
  }
  wbs(point: At = {}): Promise<Wbs> {
    return this.get("/api/1/wbs", { ...point });
  }
  explain(activity: string, field: string, point: At & { project?: number } = {}): Promise<Explanation> {
    return this.get("/api/1/explain", { activity, field, ...point });
  }
  check(point: At = {}): Promise<CheckReport> {
    return this.get("/api/1/check", { ...point });
  }
  /** Every activity code type with all its values in sequence, and every activity user field. */
  fields(point: At = {}): Promise<Fields> {
    return this.get("/api/1/fields", { ...point });
  }
  /** The activities `query`'s filters find, with the columns asked for; every match unless `limit` is given. */
  query(params: QueryParams, point: At = {}): Promise<QueryReport> {
    return this.get("/api/1/query", { ...params, ...point });
  }

  // ---- A portfolio served ----
  /** The portfolio's EPS with each node's and project's figures. */
  portfolio(): Promise<PortfolioReport> {
    return this.get("/api/1/portfolio");
  }
  /** Each resource's demand across the portfolio against its supply. */
  capacity(query: { by?: "week" | "month"; resource?: string } = {}): Promise<CapacityReport> {
    return this.get("/api/1/portfolio/capacity", query);
  }
  /** Shows another of the portfolio's schedules, read-only; answers its summary. */
  async openProject(file: string): Promise<Summary> {
    return (await this.post<Summary>("/api/1/portfolio/open", { file })).answer;
  }

  /** A resource's units per week or month, actual and remaining, against its limit. */
  usage(query: { resource: string; by?: "week" | "month"; project?: number }): Promise<UsageReport> {
    const { resource, by, project } = query;
    return this.get("/api/1/usage", {
      resource,
      ...(by ? { by } : {}),
      ...(project === undefined ? {} : { project: String(project) }),
    });
  }

  /** A schedule risk analysis of what's shown; it changes nothing. */
  async risk(request: {
    inputs: RiskInputs;
    iterations?: number;
    seed?: number;
    target?: string;
    project?: number;
  }): Promise<RiskReport> {
    return (await this.post<RiskReport>("/api/1/risk", request)).answer;
  }

  // ---- Changes ----
  /**
   * Edits. A dry run with `check` that a newly failing check blocks answers
   * 422 with the report, which is given as the answer; anything else that
   * answers 422 is a refusal, as a blocked edit records nothing.
   */
  edit(request: EditRequest): Promise<Changed<EditReport>> {
    return this.post("/api/1/edit", request, request.dry_run ? isEditReport : undefined);
  }

  // ---- History ----
  log(query: { branch?: string; limit?: number; all?: boolean } = {}): Promise<HistoryLog> {
    return this.get("/api/1/history/log", query);
  }
  show(point: string): Promise<ChangesetDocument> {
    return this.get(`/api/1/history/show/${point.split("/").map(encodeURIComponent).join("/")}`);
  }
  /** What changed between two points; at most `limit` of each kind per project (25 if not given, every one for 0). */
  diff(query: { from?: string; to?: string; limit?: number } = {}): Promise<HistoryDiff> {
    return this.get("/api/1/history/diff", query);
  }
  proposals(): Promise<Proposals> {
    return this.get("/api/1/history/proposals");
  }
  /**
   * The XER at a point, and its file name. It's fetched rather than linked
   * to, since a plain link can't send the session header; save it with an
   * object URL.
   */
  async exportXer(point: At = {}): Promise<{ name: string; blob: Blob }> {
    const response = await this.send(withQuery("/api/1/history/export", { ...point }), {
      headers: this.headers(),
    });
    const disposition = response.headers.get("content-disposition") ?? "";
    const name = /filename="([^"]+)"/.exec(disposition)?.[1] ?? "schedule.xer";
    return { name, blob: await response.blob() };
  }
  revert(body: { at?: string; reason?: string; head?: string } = {}): Promise<Changed<Reverted>> {
    return this.post("/api/1/history/revert", body);
  }
  branches(): Promise<Changed<Branches>> {
    return this.post("/api/1/history/branch", { action: "list" });
  }
  branch(body: {
    action: "create" | "switch" | "delete";
    name: string;
    at?: string;
    head?: string;
  }): Promise<Changed<BranchChange>> {
    return this.post("/api/1/history/branch", body);
  }
  baselines(): Promise<Changed<Baselines>> {
    return this.post("/api/1/history/baseline", { action: "list" });
  }
  baseline(body: { action: "create" | "delete"; name: string; at?: string; head?: string }): Promise<
    Changed<BaselineChange>
  > {
    return this.post("/api/1/history/baseline", body);
  }
  merge(body: { branch: string; dry_run?: boolean; reason?: string; head?: string }): Promise<Changed<Merged>> {
    return this.post("/api/1/history/merge", body);
  }
  // ---- Layouts ----
  layouts(): Promise<Layouts> {
    return this.get("/api/1/layouts");
  }
  layout(name: string): Promise<Layout> {
    return this.get(`/api/1/layouts/${encodeURIComponent(name)}`);
  }
  /** Saves a layout under `name`, replacing any of that name; answers with it as saved. */
  async saveLayout(name: string, layout: Layout): Promise<Layout> {
    const path = `/api/1/layouts/${encodeURIComponent(name)}`;
    const response = await this.send(path, {
      method: "PUT",
      headers: { ...this.headers(), "content-type": "application/json" },
      body: JSON.stringify(layout),
    });
    return Api.json<Layout>(response, path);
  }
  async deleteLayout(name: string): Promise<void> {
    await this.send(`/api/1/layouts/${encodeURIComponent(name)}`, { method: "DELETE", headers: this.headers() });
  }
  // ---- Named filters ----
  filters(): Promise<NamedFilters> {
    return this.get("/api/1/filters");
  }
  filter(name: string): Promise<NamedFilter> {
    return this.get(`/api/1/filters/${encodeURIComponent(name)}`);
  }
  /** Saves a filter under `name`, replacing any of that name; answers with it as saved. */
  async saveFilter(name: string, filter: NamedFilter): Promise<NamedFilter> {
    const path = `/api/1/filters/${encodeURIComponent(name)}`;
    const response = await this.send(path, {
      method: "PUT",
      headers: { ...this.headers(), "content-type": "application/json" },
      body: JSON.stringify(filter),
    });
    return Api.json<NamedFilter>(response, path);
  }
  async deleteFilter(name: string): Promise<void> {
    await this.send(`/api/1/filters/${encodeURIComponent(name)}`, { method: "DELETE", headers: this.headers() });
  }

  accept(body: { proposal: string; reason?: string; head?: string }): Promise<Changed<Reviewed>> {
    return this.post("/api/1/history/accept", body);
  }
  reject(body: { proposal: string; reason: string; head?: string }): Promise<Changed<Reviewed>> {
    return this.post("/api/1/history/reject", body);
  }
}
