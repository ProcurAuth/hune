// The option table of the type `logs` (10.3h, F33; 4.33f, 9.19h). Pure data: no DOM access. Each member is optional, so a member at its
// default value leaves the panel (the write rule of 10.3h, rule 4), and the options of a new panel are `{}`.

import { row } from "./rows.js";

/** The two values of `order`, with the words of Grafana 12.4.2. */
export const LOGS_ORDER = Object.freeze([["newest_first", "Newest first"], ["oldest_first", "Oldest first"]].map(Object.freeze));

const L = (id, member, kind, label, rest) => row(id, member, kind, label, "logs", { optional: true, default: undefined, ...rest });

export default {
  type: "logs",
  groups: [["logs", "Logs"]],
  rows: [
    L("logs-time", "options.show_time", "switch", "Show timestamps", { absent: false }),
    L("logs-wrap", "options.wrap", "switch", "Wrap lines", { absent: false }),
    L("logs-details", "options.details", "switch", "Enable log details", { absent: true }),
    L("logs-order", "options.order", "radio", "Order", { items: LOGS_ORDER, absent: "newest_first" }),
  ],
};
