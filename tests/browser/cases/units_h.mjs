// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B215 (4.20, 9.15; gap paper H01, item P01): the units of the later options in a stat and in a table. The texts are the texts of
// Grafana 12.4.2 (tests/frontend/vectors/FE-06-units-h.json). The server takes the new ids, and the page shows them in the DOM as text.
import { doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const UID = "b215-units-h";

const unitOf = (name, unit) => ({ match: { by: "name", value: name }, set: { unit } });

const DOC = doc(UID, "Units of wave H", [
  section(null, [
    panel(1, "stat", "Version", ["SELECT 1234.5678 AS Version"], { grid: { x: 0, y: 0, w: 6, h: 6 }, fields: { defaults: { unit: "string" }, overrides: [] } }),
    panel(
      2,
      "table",
      "Parts",
      [
        "SELECT toUInt8(number = 0) AS done, toUInt8(number) AS mutation, toBool(number = 0) AS flag,",
        "  [1536, 0.5][number + 1] AS bytes, [0.5, 1234.5678][number + 1] AS rate, [98765432, -42][number + 1] AS duration,",
        "  [1234.5678, 0.5][number + 1] AS si, [99, 0][number + 1] AS mb",
        "FROM numbers(2)",
        "ORDER BY number",
      ],
      {
        grid: { x: 6, y: 0, w: 18, h: 6 },
        fields: {
          defaults: {},
          overrides: [
            unitOf("done", "bool"),
            unitOf("mutation", "bool_on_off"),
            unitOf("flag", "bool"),
            unitOf("bytes", "kbytes"),
            unitOf("rate", "rowsps"),
            unitOf("duration", "dtdhms"),
            unitOf("si", "sishort"),
            unitOf("mb", "decmbytes"),
          ],
        },
      },
    ),
  ]),
]);

export const cases = [
  {
    id: "B215",
    name: "the units of wave H: the server takes them, a stat shows `string` and a table shows bool, size, rate, clock and SI texts of Grafana (4.20, 9.15)",
    async run(t) {
      const r = await t.api.create("transit", DOC);
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      const p = await t.page();
      await p.goto(t.url(`d/${UID}/x`));
      await p.idle();
      await waitPanelsReady(p, [1, 2]);
      const seen = await p.eval(`(() => {
        const stat = document.querySelector('[data-testid="stat-item-1"] .stat__value');
        const table = document.querySelector('[data-testid="table-2"]');
        const rows = [...table.querySelectorAll("tbody tr[data-row]")].map((r) => [...r.children].map((c) => c.textContent));
        const head = [...table.querySelectorAll("thead th")].map((c) => c.textContent.trim());
        return { stat: stat ? stat.textContent : null, head, rows, markup: table.querySelectorAll("tbody td img, tbody td script").length };
      })()`);
      t.eq(seen.stat, "1234.5678", "the stat with the unit string shows String(v), with no rounding");
      t.eq(JSON.stringify(seen.head), JSON.stringify(["done", "mutation", "flag", "bytes", "rate", "duration", "si", "mb"]), "the columns");
      t.eq(
        JSON.stringify(seen.rows),
        JSON.stringify([
          ["True", "Off", "true", "1.50 MiB", "0.500 rows/s", "1143 d 02:50:32", "1.23 k", "99 MB"],
          ["False", "On", "false", "512 B", "1.23K rows/s", "00:00:42 ago", "500 m", "0 MB"],
        ]),
        "the cells have the texts of Grafana 12.4.2",
      );
      t.eq(seen.markup, 0, "the cells hold text only");
    },
  },
];
