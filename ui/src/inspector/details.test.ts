import { describe, expect, it } from "vitest";

import type { Fields } from "../api";
import { detailsOf, detailsQuery, listItem } from "./details";

describe("an activity's details", () => {
  it("escapes commas and backslashes in a column name", () => {
    expect(listItem("Cost, east\\west")).toBe("Cost\\, east\\\\west");
  });

  it("asks for the joined codes column when the code types aren't known", () => {
    expect(detailsQuery("A1", null)).toEqual({ text: "A1", columns: "id,resources,roles,notes,codes" });
    const details = detailsOf({ id: 1, resources: "", roles: null, notes: "", codes: "Phase=DESIGN, Area=E1" }, null);
    expect(details).toEqual({ resources: [], roles: [], codes: [{ type: "Phase", value: "DESIGN" }, { type: "Area", value: "E1" }], udfs: [], notes: [] });
  });

  it("splits notes by topic, keeping each note's own lines", () => {
    const { notes } = detailsOf({ notes: "Scope: one\ntwo\n\nRisk: three" }, null);
    expect(notes).toEqual([
      { topic: "Scope", text: "one\ntwo" },
      { topic: "Risk", text: "three" },
    ]);
  });
});

describe("the columns asked for", () => {
  const fields: Fields = {
    schema: "totalfloat.fields/1",
    activity_codes: [
      { name: "Area ", values: [] },
      { name: "Phase", values: [] },
    ],
    activity_udfs: [
      { label: "", data_type: "text" },
      { label: "Pour", data_type: "start-date" },
      { label: "Pour", data_type: "start-date" },
      { label: " Owner", data_type: "text" },
    ],
  };

  it("leaves out names the query would trim or refuse, and asks for each once", () => {
    // A refused column would refuse the whole query, and every section with it.
    expect(detailsQuery("A1", fields).columns).toBe("id,resources,roles,notes,code:Phase,udf:Pour");
  });

  it("marks a date field, to show in the date format chosen", () => {
    expect(detailsOf({ "udf:Pour": "2026-01-05 08:00" }, fields).udfs).toEqual([{ label: "Pour", value: "2026-01-05 08:00", date: true }]);
  });
});
