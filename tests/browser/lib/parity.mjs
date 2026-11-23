// The visual parity capture of 15.14c for the browser cases: it runs `tools/parity/visual.mjs` for some fixtures of
// `tests/fixtures/grafana` against the disposable Grafana, and it reads the report.
//
// The cases B137 (several queries), B148 (heatmap and histogram) and B101p (repeat) call `visualParity`. A difference that a section of 19.20
// lists is a documented difference. Any other difference is a fault. The file `tests/fixtures/parity/visual-open-faults.json` lists the faults
// that are open (each with its cause). A fault that is not in the list fails the case. A listed fault that is gone makes a note: remove its line.
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ROOT } from "./env.mjs";

export const GRAFANA_URL = "http://127.0.0.1:3010";

/** True when the disposable Grafana of the parity harness answers. */
export async function grafanaUp(url = GRAFANA_URL) {
  try {
    const r = await fetch(`${url}/api/health`, { signal: AbortSignal.timeout(2000) });
    return r.status === 200;
  } catch {
    return false;
  }
}

const OPEN = JSON.parse(readFileSync(join(ROOT, "tests/fixtures/parity/visual-open-faults.json"), "utf8"));

/** The key of a fault in the list of open faults. */
export const faultKey = (r) => `${r.fixture}|${r.panel}|${r.check}`;

/**
 * Run the capture for each entry of `jobs`: `{ file, huneUid, extra: [] }`, with `file` a name in `tests/fixtures/grafana` (with no `.json`).
 * It returns `{ ran, records, faults, documented, notCaptured }`. `t.note` and `t.assert` are those of the case.
 */
export async function visualParity(t, huneUrl, jobs, { from = "1794528000000", to = "1794546000000" } = {}) {
  if (!(await grafanaUp())) {
    t.note(`the visual capture of 15.14c needs the Grafana of ${GRAFANA_URL}: it did not run here`);
    return null;
  }
  const out = mkdtempSync(join(tmpdir(), "hune-visual-"));
  const result = { ran: 0, records: [], faults: [], documented: 0, notCaptured: [] };
  try {
    for (const job of jobs) {
      const dir = join(out, job.file);
      const run = spawnSync(
        process.execPath,
        [join(ROOT, "tools/parity/visual.mjs"), "--out", dir, "--fixture", join(ROOT, "tests/fixtures/grafana", `${job.file}.json`), "--grafana-url", GRAFANA_URL, "--hune-url", huneUrl.replace(/\/$/, ""), "--hune-uid", job.huneUid, "--from", from, "--to", to, "--grafana-undock-menu", "--timeout-s", "300", ...(job.extra ?? [])],
        { encoding: "utf8", timeout: 600000 },
      );
      let report;
      try {
        report = JSON.parse(readFileSync(join(dir, "report.json"), "utf8"));
      } catch {
        result.notCaptured.push(`${job.file}: exit ${run.status}: ${(run.stderr || run.stdout).slice(-300)}`);
        continue;
      }
      result.ran++;
      result.records.push(...report.records);
    }
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
  const different = result.records.filter((r) => r.status === "different");
  result.documented = different.filter((r) => r.class === "documented").length;
  result.faults = different.filter((r) => r.class === "fault");
  t.assert(result.notCaptured.length === 0, `the capture did not run for: ${result.notCaptured.join("; ")}`);
  const fresh = result.faults.filter((r) => !(faultKey(r) in OPEN));
  t.assert(fresh.length === 0, `faults that the list of open faults does not have (${fresh.length}): ${fresh.slice(0, 6).map((r) => `${faultKey(r)} ${JSON.stringify(r.detail).slice(0, 160)}`).join(" || ")}`);
  const seen = new Set(result.faults.map(faultKey));
  const ranFixtures = new Set(result.records.map((r) => r.fixture));
  const gone = Object.keys(OPEN).filter((k) => ranFixtures.has(k.split("|")[0]) && !seen.has(k));
  if (gone.length) t.note(`open faults that are gone (remove them from tests/fixtures/parity/visual-open-faults.json): ${gone.join(", ")}`);
  t.note(`visual capture: ${result.ran} dashboards, ${result.records.length} checks, ${result.documented} documented differences, ${result.faults.length} open faults`);
  return result;
}
