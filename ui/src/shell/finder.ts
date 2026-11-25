// What the grid's built filters and grouping read: the file's activity code
// types and user fields from `GET /api/1/fields`, every one of them, and
// `query` on the server.

import type { Api, Fields } from "../api";
import type { GridFinder } from "../grid/Grid";

/** The finder for the file's code types and fields, querying through `api`. */
export function finderOf(api: Api, fields: Fields): GridFinder {
  // Project code types can share a name across projects: one entry each
  // name, with the values of all of them, the first's sequence first.
  const byName = new Map<string, string[]>();
  for (const type of fields.activity_codes) {
    const values = byName.get(type.name) ?? [];
    for (const v of type.values) if (!values.includes(v.value)) values.push(v.value);
    byName.set(type.name, values);
  }
  const codes = [...byName].map(([name, values]) => ({ name, values }));
  return {
    groups: [
      ...codes.map((t) => ({
        field: `code:${t.name}`,
        title: t.name,
        kind: "code" as const,
        // The code type's own sequence, as the server lists its values.
        order: new Map(t.values.map((v, k) => [v, k])),
      })),
      ...fields.activity_udfs.map((f) => ({ field: `udf:${f.label}`, title: f.label, kind: "udf" as const })),
    ],
    codes: codes.filter((t) => t.values.length > 0).map((t) => ({ type: t.name, values: t.values })),
    query: (params) => api.query(params),
    saved: {
      list: async () => (await api.filters()).filters.map((f) => f.name),
      read: async (name) => {
        const filter = await api.filter(name);
        return { built: filter.built ?? {}, any: filter.any ?? [] };
      },
      save: async (name, { built, any }) => {
        await api.saveFilter(name, { schema: "totalfloat.filter/1", built, ...(any.length > 0 ? { any } : {}) });
      },
      remove: (name) => api.deleteFilter(name),
    },
  };
}
