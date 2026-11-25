import { describe, expect, it } from "vitest";

import type { Api, Fields } from "../api";
import { finderOf } from "./finder";

describe("the grid's finder", () => {
  it("offers each code type and user field to group by, and code values to filter by", async () => {
    const fields = {
      schema: "totalfloat.fields/1",
      activity_codes: [
        {
          name: "Phase",
          values: [
            { value: "DESIGN", description: "" },
            { value: "BUILD", description: "" },
          ],
        },
        { name: "Empty", values: [] },
      ],
      activity_udfs: [{ label: "Crew size", data_type: "integer" }],
    } satisfies Fields;
    const asked: unknown[] = [];
    const api = { query: (params: unknown) => (asked.push(params), Promise.resolve({})) } as unknown as Api;
    const finder = finderOf(api, fields);
    expect(finder.groups.map((g) => [g.field, g.kind])).toEqual([
      ["code:Phase", "code"],
      ["code:Empty", "code"],
      ["udf:Crew size", "udf"],
    ]);
    expect([...(finder.groups[0]?.order ?? [])]).toEqual([
      ["DESIGN", 0],
      ["BUILD", 1],
    ]);
    expect(finder.codes).toEqual([{ type: "Phase", values: ["DESIGN", "BUILD"] }]);
    await finder.query({ code: "Phase=DESIGN" });
    expect(asked).toEqual([{ code: "Phase=DESIGN" }]);
  });

  it("offers a code type named in more than one project once, with all its values", () => {
    const type = (values: string[]) => ({
      name: "Phase",
      values: values.map((value) => ({ value, description: "" })),
    });
    const fields = {
      schema: "totalfloat.fields/1",
      activity_codes: [type(["DESIGN", "BUILD"]), type(["BUILD", "COMMISSION"])],
      activity_udfs: [],
    } satisfies Fields;
    const finder = finderOf({} as Api, fields);
    expect(finder.groups.map((g) => g.field)).toEqual(["code:Phase"]);
    expect([...(finder.groups[0]?.order ?? [])]).toEqual([
      ["DESIGN", 0],
      ["BUILD", 1],
      ["COMMISSION", 2],
    ]);
    expect(finder.codes).toEqual([{ type: "Phase", values: ["DESIGN", "BUILD", "COMMISSION"] }]);
  });

  it("offers every value and field, past the 100 inspect lists", () => {
    const values = Array.from({ length: 150 }, (_, k) => `V${String(149 - k).padStart(3, "0")}`);
    const fields = {
      schema: "totalfloat.fields/1",
      activity_codes: [{ name: "Many", values: values.map((value) => ({ value, description: "" })) }],
      activity_udfs: Array.from({ length: 120 }, (_, k) => ({ label: `Field ${k}`, data_type: "text" })),
    } satisfies Fields;
    const finder = finderOf({} as Api, fields);
    // Every value offered to filter by, in sequence.
    expect(finder.codes).toEqual([{ type: "Many", values }]);
    // Every field under Group, after the code type.
    expect(finder.groups).toHaveLength(121);
    expect(finder.groups.at(-1)).toMatchObject({ field: "udf:Field 119", kind: "udf" });
    // Every band has its place in the code type's sequence.
    const order = finder.groups[0]?.order;
    expect(order?.size).toBe(150);
    expect(order?.get("V149")).toBe(0);
    expect(order?.get("V000")).toBe(149);
  });
});
