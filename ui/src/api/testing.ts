// Answers for tests, built on the golden examples so they keep every field
// the server sends.
import { golden } from "./schemas/golden";
import type { Inspect, ProjectSummary } from "./types";

/** The fields of a project the app reads. */
type Shown = Pick<ProjectSummary, "id" | "short_name" | "name" | "data_date" | "finish" | "activities">;

/** `inspect`'s report on `file` with `projects`, the rest as in the golden summary. */
export function inspectOf(file: string, projects: Shown[]): Inspect {
  const base = golden["summary-1"].inspect;
  const template = base.projects[0];
  if (!template) throw new Error("the golden summary has no project");
  return { ...base, file, projects: projects.map((project) => ({ ...template, ...project })) };
}
