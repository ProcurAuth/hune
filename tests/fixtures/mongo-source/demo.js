// Synthetic demo database of the MongoDB source (8b.13, 16.9b). demo.sh runs this file with mongosh.
//
// It makes the database `analytics` with three collections and the user `hune_ro` (the role read on analytics
// and no other role, SCRAM-SHA-256 only). All data is synthetic and has no random part.
//   node_load    a time series of metrics: 20 nodes, one sample for each node every 60 s, for MGS_DEMO_DAYS days
//                that end now. Fields: ts, node, region, cpu, mem, load1, net_in.
//   node_events  events of the nodes: ts, node, level, code, message, attrs.
//   orders       orders with a Decimal128 total, a status, and an array of items.
// Variables (mandatory): MGS_DEMO_RO_PASSWORD. Optional: MGS_DEMO_DAYS (1 to 30, default 2).
// The script stops when the database `analytics` has a collection already.

const pw = process.env.MGS_DEMO_RO_PASSWORD;
if (!pw) throw new Error("demo.js: MGS_DEMO_RO_PASSWORD is not set");
const days = Number(process.env.MGS_DEMO_DAYS || "2");
if (!Number.isInteger(days) || days < 1 || days > 30) throw new Error("demo.js: MGS_DEMO_DAYS must be from 1 to 30");

const analytics = db.getSiblingDB("analytics");
if (analytics.getCollectionNames().length > 0) {
  throw new Error("demo.js: the database analytics is not empty; use a new server");
}

const NODES = 20;
const STEP_MS = 60000;
const end = Math.floor(Date.now() / STEP_MS) * STEP_MS;
const samples = days * 24 * 60;
const start = end - (samples - 1) * STEP_MS;
const two = (n) => (n < 10 ? "0" + n : String(n));

let docs = [];
for (let s = 0; s < samples; s++) {
  const t = start + s * STEP_MS;
  for (let n = 0; n < NODES; n++) {
    const phase = (s / 1440) * 2 * Math.PI + n;
    const cpu = Math.round((45 + 35 * Math.sin(phase) + ((s * 7 + n * 13) % 10)) * 100) / 100;
    docs.push({
      ts: new Date(t),
      node: "node-" + two(n),
      region: n % 2 === 0 ? "eu-1" : "us-1",
      cpu: cpu,
      mem: Math.round((50 + 20 * Math.sin(phase / 2) + ((s * 3 + n) % 8)) * 100) / 100,
      load1: Math.round((cpu / 25) * 100) / 100,
      net_in: Long(Math.floor((1000 + 900 * Math.sin(phase * 3)) * 1000)),
    });
  }
  if (docs.length >= 10000) {
    analytics.node_load.insertMany(docs, { ordered: false });
    docs = [];
  }
}
if (docs.length > 0) analytics.node_load.insertMany(docs, { ordered: false });
analytics.node_load.createIndex({ ts: 1 });
analytics.node_load.createIndex({ node: 1, ts: 1 });

const levels = ["info", "info", "info", "warn", "error"];
const events = [];
for (let i = 0; i < days * 24 * 12; i++) {
  events.push({
    ts: new Date(start + i * 5 * STEP_MS),
    node: "node-" + two((i * 7) % NODES),
    level: levels[i % 5],
    code: i % 7,
    message: "synthetic event " + i,
    attrs: { shard: i % 4, retry: i % 3 === 0 },
  });
}
analytics.node_events.insertMany(events, { ordered: false });
analytics.node_events.createIndex({ ts: 1 });

const status = ["new", "paid", "shipped", "refunded"];
const orders = [];
for (let i = 0; i < days * 24 * 6; i++) {
  orders.push({
    placed_at: new Date(start + i * 10 * STEP_MS),
    customer: "customer-" + two(i % 17),
    status: status[i % 4],
    total: Decimal128(String(10 + (i % 50)) + "." + two((i * 7) % 100)),
    items: [{ sku: "sku-" + (i % 9), qty: 1 + (i % 3) }],
  });
}
analytics.orders.insertMany(orders, { ordered: false });
analytics.orders.createIndex({ placed_at: 1 });

db.getSiblingDB("admin").createUser({
  user: "hune_ro",
  pwd: pw,
  roles: [{ role: "read", db: "analytics" }],
  mechanisms: ["SCRAM-SHA-256"],
});
print("demo.js: node_load " + analytics.node_load.countDocuments({}) + ", node_events " + events.length +
  ", orders " + orders.length);
