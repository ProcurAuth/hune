// The session member `transform_max_cells` (6.10) for the front tests. The page keeps no copy of the cap of the cells: it reads
// `app.session` (9.21a, rule 13). This helper gives the session the default of the key `server.transform_max_cells`, read from the
// key table of the server (crates/hune/src/config/keys.rs), so the tests and the server share one source.
import { readFileSync } from "node:fs";
import { app } from "../../web/js/config.js";

const keys = readFileSync(new URL("../../crates/hune/src/config/keys.rs", import.meta.url), "utf8");
const read = (name) => Number(new RegExp(`pub const ${name}: u32 = ([0-9_]+);`).exec(keys)[1].replaceAll("_", ""));

/** The default and the fixed upper bound of `server.transform_max_cells` (13.8). */
export const DEFAULT_CELLS = read("TRANSFORM_MAX_CELLS_DEFAULT");
export const BOUND_CELLS = read("TRANSFORM_MAX_CELLS_BOUND");

/** Gives the session the member `transform_max_cells` = `cells`. `undefined` gives a session with no such member. */
export function setCells(cells) {
  app.session = cells === undefined ? {} : { transform_max_cells: cells };
}

/** Runs `fn` with the member `cells`, then gives the session the default again. */
export function withCells(cells, fn) {
  setCells(cells);
  try {
    return fn();
  } finally {
    setCells(DEFAULT_CELLS);
  }
}

setCells(DEFAULT_CELLS);
