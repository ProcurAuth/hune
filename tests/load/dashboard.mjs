// SPDX-License-Identifier: MIT OR Apache-2.0
// Load driver of 16.16: the Node equivalent of `tests/load/dashboard.js` (k6 is not available offline).
//
// Scenario (16.16): V viewers, each on the synthetic 45-panel dashboard. A viewer loads the panels
// with at most 6 queries in flight (as a browser does), then refreshes every R seconds for D seconds.
// Each panel query is `SELECT number FROM numbers(100)`.
//
// Two phases run one after the other with the same viewers, the same concurrency and the same refresh:
//   1. baseline: the SQL goes straight to ClickHouse over HTTP, with the settings of 8.3;
//   2. load:     the panel query route of Hune (6.20), and the resident memory of the Hune process.
//
// Use: node tests/load/dashboard.mjs --hune URL --clickhouse URL --pid PID --out DIR [options]
// Exit code: 0 when every criterion passes, 1 when one fails, 2 for a usage or set-up error.
import http from 'node:http';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import { join } from 'node:path';

const OPTIONS = {
  hune: { type: 'string', required: true, help: 'base URL of Hune, for example http://127.0.0.1:18080' },
  clickhouse: { type: 'string', required: true, help: 'base URL of ClickHouse' },
  pid: { type: 'int', required: true, help: 'process id of Hune, for VmRSS' },
  out: { type: 'string', required: true, help: 'output directory' },
  uid: { type: 'string', default: 'load45', help: 'uid of the dashboard' },
  viewers: { type: 'int', default: 5, help: 'number of viewers' },
  refresh: { type: 'float', default: 30, help: 'refresh period in seconds' },
  duration: { type: 'float', default: 300, help: 'duration of the load phase in seconds' },
  'baseline-duration': { type: 'float', default: 120, help: 'duration of the baseline phase in seconds' },
  inflight: { type: 'int', default: 6, help: 'queries in flight for each viewer' },
  stagger: { type: 'bool', default: true, help: 'spread the first load of the viewers over one refresh period' },
  'ch-user': { type: 'string', default: 'hune', help: 'ClickHouse user' },
  'ch-password-file': { type: 'string', required: true, help: 'file with the ClickHouse password' },
  'ch-database': { type: 'string', default: 'analytics', help: 'ClickHouse database' },
  'mem-interval': { type: 'float', default: 10, help: 'seconds between two VmRSS samples' },
  'mem-tail': { type: 'float', default: 0.6667, help: 'fraction of the load phase that the memory criterion covers' },
  'max-overhead-ms': { type: 'float', default: 50, help: 'pass limit of p95(hune) - p95(direct), in ms' },
  'max-mem-growth': { type: 'float', default: 5, help: 'pass limit of the memory growth, in percent' },
  label: { type: 'string', default: 'run', help: 'name of the run in the output' },
};

function parseArgs(argv) {
  const values = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith('--')) throw new Error(`unexpected argument ${arg}`);
    let name = arg.slice(2);
    let raw;
    const eq = name.indexOf('=');
    if (eq >= 0) {
      raw = name.slice(eq + 1);
      name = name.slice(0, eq);
    }
    const spec = OPTIONS[name];
    if (!spec) throw new Error(`unknown option --${name}`);
    if (raw === undefined) {
      i += 1;
      raw = argv[i];
    }
    if (raw === undefined) throw new Error(`option --${name} needs a value`);
    if (spec.type === 'int') {
      if (!/^-?\d+$/.test(raw)) throw new Error(`option --${name}: integer expected, value ${raw}`);
      values[name] = Number(raw);
    } else if (spec.type === 'float') {
      if (!Number.isFinite(Number(raw))) throw new Error(`option --${name}: number expected, value ${raw}`);
      values[name] = Number(raw);
    } else if (spec.type === 'bool') {
      if (raw !== 'true' && raw !== 'false') throw new Error(`option --${name}: true or false expected, value ${raw}`);
      values[name] = raw === 'true';
    } else {
      values[name] = raw;
    }
  }
  for (const [name, spec] of Object.entries(OPTIONS)) {
    if (values[name] === undefined) {
      if (spec.required) throw new Error(`option --${name} is necessary (${spec.help})`);
      values[name] = spec.default;
    }
  }
  return values;
}

let opt;
try {
  opt = parseArgs(process.argv.slice(2));
} catch (e) {
  console.error(`dashboard.mjs: ${e.message}`);
  console.error('options:');
  for (const [name, spec] of Object.entries(OPTIONS)) {
    console.error(`  --${name}  ${spec.help}${spec.default !== undefined ? ` (default ${spec.default})` : ''}`);
  }
  process.exit(2);
}

const SQL = 'SELECT number FROM numbers(100)';
const huneUrl = new URL(opt.hune);
const chUrl = new URL(opt.clickhouse);
const chPassword = readFileSync(opt['ch-password-file'], 'utf8').replace(/\n$/, '');
const REQUEST_TIMEOUT_MS = 30000;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// One keep-alive agent for each viewer, with at most `inflight` sockets: the 6 connections of a browser.
function makeAgent() {
  return new http.Agent({ keepAlive: true, maxSockets: opt.inflight, keepAliveMsecs: 1000 });
}

// Send one request and read the whole body. Return { status, body, ms }. `ms` is the time in the client from
// the call to the end of the body. A network error or a timeout gives status 0.
function request(agent, base, method, path, headers, payload) {
  return new Promise((resolve) => {
    const t0 = performance.now();
    const req = http.request(
      { agent, hostname: base.hostname, port: base.port, method, path, headers, timeout: REQUEST_TIMEOUT_MS },
      (res) => {
        const chunks = [];
        res.on('data', (c) => chunks.push(c));
        res.on('end', () => {
          const ms = performance.now() - t0;
          resolve({ status: res.statusCode, body: Buffer.concat(chunks), ms });
        });
        res.on('error', (err) => resolve({ status: 0, body: Buffer.alloc(0), ms: performance.now() - t0, error: err.message }));
      },
    );
    req.on('timeout', () => req.destroy(new Error('timeout')));
    req.on('error', (err) => resolve({ status: 0, body: Buffer.alloc(0), ms: performance.now() - t0, error: err.message }));
    if (payload !== undefined) req.write(payload);
    req.end();
  });
}

// ---- Dashboard check -------------------------------------------------------------------------------------

async function getJson(agent, path) {
  const r = await request(agent, huneUrl, 'GET', path, { accept: 'application/json' });
  if (r.status !== 200) throw new Error(`GET ${path}: status ${r.status} ${r.body.toString('utf8').slice(0, 300)} ${r.error ?? ''}`);
  return JSON.parse(r.body.toString('utf8'));
}

async function findDashboard() {
  const agent = makeAgent();
  try {
    const list = await getJson(agent, '/api/dashboards');
    const item = list.items.find((i) => i.uid === opt.uid);
    if (!item) throw new Error(`the list of dashboards does not hold ${opt.uid}`);
    if (item.state !== 'valid') {
      throw new Error(`the dashboard ${opt.uid} is not valid: ${JSON.stringify(item.errors ?? item).slice(0, 600)}`);
    }
    const doc = await getJson(agent, `/api/dashboards/${opt.uid}`);
    const panels = [];
    const sections = doc.document?.sections ?? doc.sections ?? [];
    for (const s of sections) for (const p of s.panels) panels.push(p.id);
    if (panels.length === 0) throw new Error('no panel found in the dashboard answer');
    return { etag: item.etag, panels };
  } finally {
    agent.destroy();
  }
}

// ---- Query senders ---------------------------------------------------------------------------------------

function huneQuery(agent, dash, panelId) {
  const to = Date.now();
  const body = JSON.stringify({
    etag: dash.etag,
    from_ms: to - 6 * 3600 * 1000,
    to_ms: to,
    max_data_points: 1200,
    variables: {},
  });
  return request(
    agent,
    huneUrl,
    'POST',
    `/api/dashboards/${opt.uid}/panels/${panelId}/query`,
    { 'content-type': 'application/json', 'content-length': Buffer.byteLength(body), accept: 'application/json' },
    body,
  ).then((r) => {
    if (r.status !== 200) return { ...r, ok: false, why: `status ${r.status} ${r.error ?? r.body.toString('utf8').slice(0, 200)}` };
    try {
      const frame = JSON.parse(r.body.toString('utf8'));
      if (frame.rows !== 100 || frame.values?.[0]?.length !== 100) return { ...r, ok: false, why: `bad frame rows=${frame.rows}` };
      return { ...r, ok: true, serverMs: frame.stats?.elapsed_ms };
    } catch (e) {
      return { ...r, ok: false, why: `bad JSON ${e.message}` };
    }
  });
}

// The settings of 8.3, as the client of Hune sends them.
const CH_QUERY = new URLSearchParams({
  database: opt['ch-database'],
  default_format: 'JSONCompactEachRowWithNamesAndTypes',
  max_execution_time: '60',
  timeout_overflow_mode: 'throw',
  max_result_rows: '100000',
  max_result_bytes: '33554432',
  result_overflow_mode: 'throw',
  max_query_size: '1048576',
  wait_end_of_query: '1',
  cancel_http_readonly_queries_on_client_close: '1',
  output_format_json_quote_64bit_integers: '1',
  output_format_json_quote_decimals: '1',
  output_format_json_quote_denormals: '1',
  output_format_json_validate_utf8: '1',
  date_time_output_format: 'unix_timestamp',
});

let queryCounter = 0;
function directQuery(agent) {
  queryCounter += 1;
  const q = new URLSearchParams(CH_QUERY);
  q.set('query_id', `load-direct-${process.pid}-${queryCounter}`);
  q.set('log_comment', '{"app":"load","kind":"direct"}');
  return request(
    agent,
    chUrl,
    'POST',
    `/?${q.toString().replace(/\+/g, '%20')}`,
    {
      'x-clickhouse-user': opt['ch-user'],
      'x-clickhouse-key': chPassword,
      'content-type': 'text/plain; charset=utf-8',
      'content-length': Buffer.byteLength(SQL),
    },
    SQL,
  ).then((r) => {
    if (r.status !== 200) return { ...r, ok: false, why: `status ${r.status} ${r.error ?? r.body.toString('utf8').slice(0, 200)}` };
    // Names line, types line, 100 rows.
    const lines = r.body.toString('utf8').split('\n').filter((l) => l.length > 0);
    if (lines.length !== 102) return { ...r, ok: false, why: `bad answer, ${lines.length} lines` };
    return { ...r, ok: true };
  });
}

// ---- Viewers ---------------------------------------------------------------------------------------------

// Run `fn(item)` for each item with at most `limit` calls in flight.
async function pool(items, limit, fn) {
  let next = 0;
  const workers = [];
  for (let w = 0; w < Math.min(limit, items.length); w += 1) {
    workers.push(
      (async () => {
        while (next < items.length) {
          const item = items[next];
          next += 1;
          await fn(item);
        }
      })(),
    );
  }
  await Promise.all(workers);
}

// One phase. `send(agent, panelId)` returns { ok, ms, why, serverMs }. Return the samples and the errors.
async function runPhase(name, send, panels, durationS) {
  const samples = [];
  const serverMs = [];
  const errors = [];
  let loads = 0;
  let errorCount = 0;
  const tStart = performance.now();
  const deadline = tStart + durationS * 1000;
  const periodMs = opt.refresh * 1000;
  const viewers = [];
  for (let v = 0; v < opt.viewers; v += 1) {
    viewers.push(
      (async () => {
        const agent = makeAgent();
        // A viewer opens the page at a random moment of the period: browsers are not in step.
        if (opt.stagger && opt.viewers > 1) await sleep(Math.random() * periodMs);
        try {
          while (performance.now() < deadline) {
            const cycle = performance.now();
            await pool(panels, opt.inflight, async (id) => {
              const r = await send(agent, id);
              samples.push(r.ms);
              if (r.ok) {
                if (r.serverMs !== undefined && r.serverMs !== null) serverMs.push(r.serverMs);
              } else if (errors.length < 50) {
                errors.push(`${name} viewer ${v} panel ${id}: ${r.why}`);
              }
              if (!r.ok) errorCount += 1;
            });
            loads += 1;
            const wait = cycle + periodMs - performance.now();
            if (wait > 0) await sleep(Math.min(wait, Math.max(0, deadline - performance.now())));
          }
        } finally {
          agent.destroy();
        }
      })(),
    );
  }
  await Promise.all(viewers);
  const wall = (performance.now() - tStart) / 1000;
  return { name, samples, serverMs, errors, errorCount, loads, wall };
}

function percentile(sorted, p) {
  if (sorted.length === 0) return NaN;
  // Nearest-rank method.
  const rank = Math.max(1, Math.ceil((p / 100) * sorted.length));
  return sorted[rank - 1];
}

function stats(values) {
  const s = [...values].sort((a, b) => a - b);
  return {
    n: s.length,
    min: s[0],
    p50: percentile(s, 50),
    p95: percentile(s, 95),
    p99: percentile(s, 99),
    max: s[s.length - 1],
    mean: s.reduce((a, b) => a + b, 0) / (s.length || 1),
  };
}

// ---- Memory sampling -------------------------------------------------------------------------------------

function readRssKb(pid) {
  try {
    const text = readFileSync(`/proc/${pid}/status`, 'utf8');
    const m = /^VmRSS:\s+(\d+) kB/m.exec(text);
    return m ? Number(m[1]) : null;
  } catch {
    return null;
  }
}

function startMemorySampler(pid, intervalS) {
  const points = [];
  const t0 = performance.now();
  const take = () => {
    const kb = readRssKb(pid);
    if (kb !== null) points.push({ t: (performance.now() - t0) / 1000, kb });
  };
  take();
  const timer = setInterval(take, intervalS * 1000);
  return {
    stop() {
      clearInterval(timer);
      take();
      return points;
    },
  };
}

function memoryGrowth(points, tailFraction) {
  if (points.length < 4) return { ok: false, reason: 'fewer than 4 samples' };
  const total = points[points.length - 1].t;
  const from = total * (1 - tailFraction);
  const tail = points.filter((p) => p.t >= from);
  if (tail.length < 4) return { ok: false, reason: 'fewer than 4 samples in the tail' };
  const k = Math.min(3, Math.floor(tail.length / 2));
  const mean = (a) => a.reduce((x, p) => x + p.kb, 0) / a.length;
  const start = mean(tail.slice(0, k));
  const end = mean(tail.slice(-k));
  const peak = Math.max(...tail.map((p) => p.kb));
  return {
    ok: true,
    tailFromS: from,
    startKb: start,
    endKb: end,
    peakKb: peak,
    growthPct: ((end - start) / start) * 100,
    peakGrowthPct: ((peak - start) / start) * 100,
  };
}

// ---- Main ------------------------------------------------------------------------------------------------

const f1 = (x) => (Number.isFinite(x) ? x.toFixed(2) : 'n/a');

async function main() {
  console.log(`== ${opt.label}: ${opt.viewers} viewers, refresh ${opt.refresh} s, load ${opt.duration} s, baseline ${opt['baseline-duration']} s`);
  if (readRssKb(opt.pid) === null) throw new Error(`cannot read /proc/${opt.pid}/status`);

  const dash = await findDashboard();
  console.log(`dashboard ${opt.uid}: valid, ${dash.panels.length} panels, etag ${dash.etag.slice(0, 12)}...`);

  // Warm-up: one query for each panel on both paths, not counted.
  {
    const agent = makeAgent();
    await pool(dash.panels, opt.inflight, async (id) => {
      const a = await huneQuery(agent, dash, id);
      if (!a.ok) throw new Error(`warm-up through Hune failed: ${a.why}`);
      const b = await directQuery(agent);
      if (!b.ok) throw new Error(`warm-up direct failed: ${b.why}`);
    });
    agent.destroy();
  }

  console.log('phase 1: baseline, direct to ClickHouse');
  const base = await runPhase('direct', (agent) => directQuery(agent), dash.panels, opt['baseline-duration']);

  console.log('phase 2: load through Hune');
  const rssBefore = readRssKb(opt.pid);
  const sampler = startMemorySampler(opt.pid, opt['mem-interval']);
  const load = await runPhase('hune', (agent, id) => huneQuery(agent, dash, id), dash.panels, opt.duration);
  const memPoints = sampler.stop();
  const growth = memoryGrowth(memPoints, opt['mem-tail']);

  const sBase = stats(base.samples);
  const sLoad = stats(load.samples);
  const sServer = stats(load.serverMs);
  const overheadP95 = sLoad.p95 - sBase.p95;

  const criteria = {
    no_error: base.errorCount === 0 && load.errorCount === 0,
    overhead_p95: overheadP95 <= opt['max-overhead-ms'],
    memory_growth: growth.ok && growth.growthPct < opt['max-mem-growth'],
  };
  const pass = Object.values(criteria).every(Boolean);

  const result = {
    label: opt.label,
    params: {
      viewers: opt.viewers,
      refresh_s: opt.refresh,
      duration_s: opt.duration,
      baseline_duration_s: opt['baseline-duration'],
      inflight: opt.inflight,
      stagger: opt.stagger,
      panels: dash.panels.length,
    },
    baseline: { ...sBase, errors: base.errorCount, page_loads: base.loads, wall_s: base.wall },
    hune: { ...sLoad, errors: load.errorCount, page_loads: load.loads, wall_s: load.wall },
    hune_server_elapsed_ms: sServer,
    overhead_p95_ms: overheadP95,
    overhead_p50_ms: sLoad.p50 - sBase.p50,
    memory: { rss_before_kb: rssBefore, samples: memPoints, tail: growth },
    criteria,
    pass,
    error_samples: [...base.errors, ...load.errors].slice(0, 20),
  };
  mkdirSync(opt.out, { recursive: true });
  writeFileSync(join(opt.out, `${opt.label}.json`), JSON.stringify(result, null, 2) + '\n');

  const row = (name, s, errors) =>
    `${name.padEnd(10)} ${String(s.n).padStart(7)} ${f1(s.p50).padStart(8)} ${f1(s.p95).padStart(8)} ${f1(s.p99).padStart(8)} ${f1(s.max).padStart(9)} ${String(errors).padStart(7)}`;
  console.log('');
  console.log('phase      samples  p50 ms   p95 ms   p99 ms    max ms  errors');
  console.log(row('direct', sBase, base.errorCount));
  console.log(row('hune', sLoad, load.errorCount));
  console.log(`hune server-side stats.elapsed_ms: p50 ${f1(sServer.p50)}, p95 ${f1(sServer.p95)}, p99 ${f1(sServer.p99)}`);
  console.log(`overhead p95 = ${f1(overheadP95)} ms (limit ${opt['max-overhead-ms']}), p50 = ${f1(result.overhead_p50_ms)} ms`);
  if (growth.ok) {
    console.log(
      `memory VmRSS: before ${rssBefore} kB, tail from ${f1(growth.tailFromS)} s: start ${f1(growth.startKb)} kB, end ${f1(growth.endKb)} kB, ` +
        `peak ${growth.peakKb} kB, growth ${f1(growth.growthPct)} % (limit ${opt['max-mem-growth']}), peak growth ${f1(growth.peakGrowthPct)} %`,
    );
  } else {
    console.log(`memory: no result (${growth.reason})`);
  }
  for (const e of result.error_samples) console.log(`error sample: ${e}`);
  console.log(`criterion no error:        ${criteria.no_error ? 'PASS' : 'FAIL'}`);
  console.log(`criterion overhead p95:    ${criteria.overhead_p95 ? 'PASS' : 'FAIL'}`);
  console.log(`criterion memory growth:   ${criteria.memory_growth ? 'PASS' : 'FAIL'}`);
  console.log(`result ${opt.label}: ${pass ? 'PASS' : 'FAIL'}`);
  return pass ? 0 : 1;
}

main().then(
  (code) => process.exit(code),
  (e) => {
    console.error(`dashboard.mjs: ${e.stack ?? e.message}`);
    process.exit(2);
  },
);
