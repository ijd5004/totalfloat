// The API's types, generated from docs/schemas/ (src/api/schemas/, by
// `pnpm gen`) and only renamed here.

// The client's types come from `Answers` below, which the golden examples
// are checked against, so a misnamed type here fails the type check.
export type Rows = Answers["rows-1"];
export type ErrorDocument = Answers["error-1"];
export type { Problem } from "./schemas/error-1";
export type ServerEvent = Answers["event-1"][number];
export type HistoryLog = Answers["history-log-1"];
export type ChangesetDocument = Answers["changeset-1"];
export type HistoryDiff = Answers["history-diff-1"];
export type Proposals = Answers["history-proposals-1"];
export type BranchChange = Answers["history-branch-1"];
export type Branches = Answers["history-branches-1"];
export type BaselineChange = Answers["history-baseline-1"];
export type Baselines = Answers["history-baselines-1"];
export type Merged = Answers["history-merge-1"];
export type Reverted = Answers["history-revert-1"];
export type Reviewed = Answers["history-review-1"];
export type { Edit } from "./schemas/command-1";
export type Layout = Answers["layout-1"];

/** `GET /api/1/layouts`: the layouts saved beside the schedule, by name. */
export type Layouts = import("./schemas/layouts-1").Listing;

/** A filter saved by name beside the schedule: `query`'s parameters. */
export type NamedFilter = Answers["filter-1"];
/** `GET /api/1/filters`: the named filters saved beside the schedule. */
export type NamedFilters = import("./schemas/filters-1").Listing;

/** `GET /api/1/summary`: what's served, and `inspect`'s report on it. */
export type Summary = Answers["summary-1"];
/** `inspect`'s report on what's served. */
export type Inspect = Summary["inspect"];
/** One project in the summary, as `inspect --json` reports it. */
export type ProjectSummary = Inspect["projects"][number];
/** An error as the server reports it, inside `totalfloat.error/1`. */
export type Refused = import("./schemas/summary-1").CliError;

/** `GET /api/1/fields`: every activity code type with all its values, and every activity user field. */
export type Fields = Answers["fields-1"];

/** `GET /api/1/links`: every relationship, one array per field. */
export type Links = Answers["links-1"];

/** `GET /api/1/wbs`: each project's WBS tree. */
export type Wbs = Answers["wbs-1"];
/** One project's WBS. */
export type ProjectWbs = Wbs["projects"][number];
/** A WBS node and the nodes under it. */
export type WbsNode = import("./schemas/wbs-1").Node;

/** `GET /api/1/explain`: `explain --json`'s report. */
export type Explanation = Answers["explain-1"];
/** One step back along an explanation: one of an activity's dates, and what set it. */
export type ExplainStep = Explanation["chains"][number]["steps"][number];
/** What set one date in an explanation. */
export type Cause = NonNullable<ExplainStep["cause"]>;

/** `GET /api/1/check`: `check --json`'s report. */
export type CheckReport = Answers["check-1"];

/** `GET /api/1/portfolio`: `portfolio show --json` of the portfolio served. */
export type PortfolioReport = Answers["portfolio-show-1"];
/** `GET /api/1/portfolio/capacity`: `portfolio capacity --json`. */
export type CapacityReport = Answers["portfolio-capacity-1"];

/** `POST /api/1/risk`: `risk --json`'s report. */
export type RiskReport = Answers["risk-analysis-1"];
/** What `POST /api/1/risk` takes, as `totalfloat.risk-inputs/1`. */
export type RiskInputs = import("./schemas/risk-inputs-1").TotalfloatRiskInputs1;

/** `GET /api/1/usage`: `query --usage --json`, a resource's units per period against its limit. */
export type UsageReport = import("./schemas/usage-1").TotalfloatUsage1;
/** One period of a usage report. */
export type UsagePeriod = UsageReport["periods"][number];

/** `GET /api/1/query`: `query --json`'s report, with every match unless a limit is given. */
export type QueryReport = Answers["query-1"];

/** `GET /api/1/query`'s parameters: `query`'s arguments, with lists separated by commas. */
export type QueryParams = Record<string, string>;

/** `POST /api/1/edit`: `edit --json`'s report. */
export type EditReport = Answers["edit-1"];
/** What an edit did to one project, against it calculated as it stood. */
export type EditImpact = EditReport["projects"][number];
/** The body of `POST /api/1/edit`. */
export type EditRequest = import("./schemas/edit-request-1").TotalfloatEditRequest1;

/**
 * Each golden example in ./golden/, by file name, and the type it's read as.
 * `pnpm gen` writes them out typed this way, so a golden example that doesn't
 * fit its type fails the type check.
 */
export interface Answers {
  "summary-1": import("./schemas/summary-1").TotalfloatSummary1;
  "rows-1": import("./schemas/rows-1").TotalfloatRows1;
  "links-1": import("./schemas/links-1").TotalfloatLinks1;
  "wbs-1": import("./schemas/wbs-1").TotalfloatWbs1;
  "explain-1": import("./schemas/explain-1").TotalfloatExplain1;
  "check-1": import("./schemas/check-1").TotalfloatCheck1;
  "query-1": import("./schemas/query-1").TotalfloatQueryJson;
  "fields-1": import("./schemas/fields-1").TotalfloatFields1;
  "edit-1": import("./schemas/edit-1").TotalfloatEdit1;
  "layouts-1": import("./schemas/layouts-1").TotalfloatLayouts1;
  "layout-1": import("./schemas/layout-1").TotalfloatLayout1;
  "filters-1": import("./schemas/filters-1").TotalfloatFilters1;
  "filter-1": import("./schemas/filter-1").TotalfloatFilter1;
  "error-1": import("./schemas/error-1").TotalfloatErrorWithJsonAndMCPToolErrors;
  "event-1": import("./schemas/event-1").TotalfloatServeEvents[];
  "history-log-1": import("./schemas/history-log-1").TotalfloatHistoryLogJson;
  "changeset-1": import("./schemas/changeset-1").TotalfloatHistoryShowJsonAndHistoryInitJson;
  "history-diff-1": import("./schemas/history-diff-1").TotalfloatHistoryDiffJson;
  "history-proposals-1": import("./schemas/history-proposals-1").TotalfloatHistoryProposalsJson;
  "history-branch-1": import("./schemas/history-branch-1").TotalfloatHistoryBranchJson;
  "history-branches-1": import("./schemas/history-branches-1").TotalfloatHistoryBranchesJson;
  "history-baseline-1": import("./schemas/history-baseline-1").TotalfloatHistoryBaselineJson;
  "history-baselines-1": import("./schemas/history-baselines-1").TotalfloatHistoryBaselinesJson;
  "history-merge-1": import("./schemas/history-merge-1").TotalfloatHistoryMergeJson;
  "history-revert-1": import("./schemas/history-revert-1").TotalfloatHistoryRevertJsonAndHistoryUndoJson;
  "history-review-1": import("./schemas/history-review-1").TotalfloatHistoryAcceptJsonAndHistoryRejectJson;
  "risk-analysis-1": import("./schemas/risk-analysis-1").TotalfloatRiskAnalysis1;
  "portfolio-show-1": import("./schemas/portfolio-show-1").TotalfloatPortfolioShow1;
  "portfolio-capacity-1": import("./schemas/portfolio-capacity-1").TotalfloatPortfolioCapacity1;
}
