// Fixture of the job `mongosource` (16.9b): the synthetic data and the users of the MongoDB source tests.
//
// Run it with apply.sh (mongosh) on a new server with no login. It holds no password: each password comes
// from an environment variable of the mongosh process. All data is synthetic and has no random part, so a
// test can compute each value.
//
// Variables (mandatory): MGS_ADMIN_PASSWORD, MGS_RO_PASSWORD, MGS_RW_PASSWORD, MGS_BAD_PASSWORD,
//   MGS_LEGACY_PASSWORD.
// Variables (optional): MGS_NODE_LOAD_DOCS (default 1000000), MGS_BIG_BYTES (default 10485760).
//   The servers that need no volume (replica set, scripting, SHA-1) set smaller values.
//
// Data:
//   analytics.node_load     N documents, one for each second from 2026-01-01T00:00:00Z. Document i has
//                           _id i, ts = base + i s, node "node-NN" with NN = i mod 20 (two digits),
//                           region "eu-1" for even NN and "us-1" for odd NN, cpu = ((i * 7919) mod 10000) / 100,
//                           mem = ((i * 104729) mod 10000) / 100, load1 = cpu / 25, net_in = Int64(i * 1500).
//   analytics.node_events   5,000 documents, one for each 30 s from the same base.
//   analytics.types         one document of each BSON type (_id 1), two documents for mixed columns
//                           (_id 2 and 3), one document with 257 fields (_id 4), one with a missing field (_id 5).
//   analytics.big           one document with a text of MGS_BIG_BYTES bytes (10 MiB by default).
//   analytics.secret        three documents. The key `collections` of a test block does not list it.
//   analytics.orders        200 orders (for the demo and the golden files).
//   analytics.fixture_marker  the last collection written. The health check reads it.
//   other.stuff             three documents (a database that the block does not list).
//
// Users (the check of each one is the check of 8b.12):
//   admin: root (role root), for the tests that set up and clean.
//   hune_ro                the contract of 8b.13: role read on analytics, SCRAM-SHA-256 only.
//   hune_rw                readWrite on analytics (MS09).
//   hune_sha1, hune_plain the contract of hune_ro with the mechanism SCRAM-SHA-1 (MS10). The community
//                           server has no PLAIN credential: hune_plain has the same mechanism as hune_sha1.
//                           The PLAIN case uses the server that offers PLAIN only (see the compose project).
//   hune_bad_*             one user for each rule that must fail (list in MGS_BAD_USERS of the script):
//     hune_bad_readwrite     role readWrite              (M2)
//     hune_bad_readany       role readAnyDatabase        (M2)
//     hune_bad_monitor       role clusterMonitor         (M2)
//     hune_bad_extradb       role read on analytics and on other (M2: a database that the list D lacks)
//     hune_bad_insert        read + a role with the action insert on analytics (M3)
//     hune_bad_anyres        read + a role with a privilege on anyResource (M3)
//     hune_bad_cluster       read + a role with a privilege on the resource cluster (M3)
//     hune_bad_authdb        read on analytics, but the user lives in the database `other` (M1)
//   A custom role is not the role `read`, so the users of M3 fail M2 too. A test looks for the check name in
//   the log line, it does not look for a line with one check only.

function need(name) {
  const v = process.env[name];
  if (v === undefined || v === "") {
    throw new Error("init.js: the variable " + name + " is not set");
  }
  return v;
}
function optInt(name, dflt) {
  const v = process.env[name];
  if (v === undefined || v === "") return dflt;
  const n = Number(v);
  if (!Number.isInteger(n) || n < 1) throw new Error("init.js: " + name + " must be a positive integer");
  return n;
}

const adminPassword = need("MGS_ADMIN_PASSWORD");
const roPassword = need("MGS_RO_PASSWORD");
const rwPassword = need("MGS_RW_PASSWORD");
const badPassword = need("MGS_BAD_PASSWORD");
const legacyPassword = need("MGS_LEGACY_PASSWORD");
const nodeLoadDocs = optInt("MGS_NODE_LOAD_DOCS", 1000000);
const bigBytes = optInt("MGS_BIG_BYTES", 10485760);

const BASE_MS = Date.UTC(2026, 0, 1, 0, 0, 0, 0);
const admin = db.getSiblingDB("admin");
const analytics = db.getSiblingDB("analytics");
const other = db.getSiblingDB("other");

function two(n) {
  return n < 10 ? "0" + n : String(n);
}

// --- data ---

{
  const batch = 10000;
  for (let start = 0; start < nodeLoadDocs; start += batch) {
    const docs = [];
    const end = Math.min(start + batch, nodeLoadDocs);
    for (let i = start; i < end; i++) {
      const nn = i % 20;
      const cpu = ((i * 7919) % 10000) / 100;
      docs.push({
        _id: i,
        ts: new Date(BASE_MS + i * 1000),
        node: "node-" + two(nn),
        region: nn % 2 === 0 ? "eu-1" : "us-1",
        cpu: cpu,
        mem: ((i * 104729) % 10000) / 100,
        load1: cpu / 25,
        net_in: Long(i * 1500),
      });
    }
    analytics.node_load.insertMany(docs, { ordered: false });
  }
  analytics.node_load.createIndex({ ts: 1 });
  analytics.node_load.createIndex({ node: 1, ts: 1 });
}

{
  const levels = ["info", "info", "info", "warn", "error"];
  const docs = [];
  for (let i = 0; i < 5000; i++) {
    docs.push({
      _id: i,
      ts: new Date(BASE_MS + i * 30000),
      node: "node-" + two(i % 20),
      level: levels[i % 5],
      code: i % 7,
      message: "synthetic event " + i,
      attrs: { shard: i % 4, retry: i % 3 === 0 },
      tags: ["t" + (i % 3), "t" + (i % 5)],
    });
  }
  analytics.node_events.insertMany(docs, { ordered: false });
  analytics.node_events.createIndex({ ts: 1 });
}

{
  const docs = [];
  const status = ["new", "paid", "shipped", "refunded"];
  for (let i = 0; i < 200; i++) {
    docs.push({
      _id: i,
      placed_at: new Date(BASE_MS + i * 600000),
      customer: "customer-" + two(i % 17),
      status: status[i % 4],
      total: Decimal128(String(10 + (i % 50)) + "." + two((i * 7) % 100)),
      items: [{ sku: "sku-" + (i % 9), qty: 1 + (i % 3) }],
    });
  }
  analytics.orders.insertMany(docs, { ordered: false });
}

{
  const wide = { _id: 4 };
  for (let i = 0; i < 257; i++) {
    wide["f" + String(i).padStart(3, "0")] = i;
  }
  analytics.types.insertMany([
    {
      _id: 1,
      t_null: null,
      t_bool: true,
      t_int32: NumberInt(2147483647),
      t_int64_max: Long("9223372036854775807"),
      t_int64_min: Long("-9223372036854775808"),
      t_double: 1.5,
      t_double_nan: NaN,
      t_double_inf: Infinity,
      t_double_ninf: -Infinity,
      t_decimal: Decimal128("12345.6700"),
      t_string: "synthetic text",
      t_date_ms: new Date(1767225600123),
      t_date_old: new Date(-86400123),
      t_objectid: ObjectId("0123456789abcdef01234567"),
      t_uuid: UUID("01234567-89ab-4def-8123-456789abcdef"),
      t_doc: { a: 1, b: { c: "deep" } },
      t_array: [1, "two", { three: 3 }, [4]],
      t_binary: BinData(0, "AQIDBA=="),
      t_regex: /^synthetic$/i,
      t_timestamp: Timestamp(1767225600, 7),
      t_minkey: MinKey(),
      t_maxkey: MaxKey(),
      t_code: Code("function () { return 1; }"),
    },
    { _id: 2, mixed_num: NumberInt(1), mixed_text: NumberInt(1) },
    { _id: 3, mixed_num: 1.5, mixed_text: "a" },
    wide,
    { _id: 5, present: 1 },
  ]);
}

analytics.big.insertOne({ _id: 1, payload: "x".repeat(bigBytes) });

analytics.secret.insertMany([
  { _id: 1, token: "synthetic-secret-1" },
  { _id: 2, token: "synthetic-secret-2" },
  { _id: 3, token: "synthetic-secret-3" },
]);

other.stuff.insertMany([{ _id: 1, v: "a" }, { _id: 2, v: "b" }, { _id: 3, v: "c" }]);

// --- roles and users ---

admin.createRole({
  role: "hune_bad_insert_role",
  privileges: [{ resource: { db: "analytics", collection: "" }, actions: ["find", "insert"] }],
  roles: [],
});
admin.createRole({
  role: "hune_bad_anyres_role",
  privileges: [{ resource: { anyResource: true }, actions: ["find"] }],
  roles: [],
});
admin.createRole({
  role: "hune_bad_cluster_role",
  privileges: [{ resource: { cluster: true }, actions: ["listDatabases"] }],
  roles: [],
});

const READ_A = { role: "read", db: "analytics" };
function user(database, name, password, roles, mechanisms) {
  database.createUser({ user: name, pwd: password, roles: roles, mechanisms: mechanisms });
}
const SHA256 = ["SCRAM-SHA-256"];

user(admin, "root", adminPassword, [{ role: "root", db: "admin" }], ["SCRAM-SHA-256", "SCRAM-SHA-1"]);
user(admin, "hune_ro", roPassword, [READ_A], SHA256);
user(admin, "hune_rw", rwPassword, [{ role: "readWrite", db: "analytics" }], SHA256);
user(admin, "hune_sha1", legacyPassword, [READ_A], ["SCRAM-SHA-1"]);
user(admin, "hune_plain", legacyPassword, [READ_A], ["SCRAM-SHA-1"]);

user(admin, "hune_bad_readwrite", badPassword, [{ role: "readWrite", db: "analytics" }], SHA256);
user(admin, "hune_bad_readany", badPassword, [{ role: "readAnyDatabase", db: "admin" }], SHA256);
user(admin, "hune_bad_monitor", badPassword, [{ role: "clusterMonitor", db: "admin" }], SHA256);
user(admin, "hune_bad_extradb", badPassword, [READ_A, { role: "read", db: "other" }], SHA256);
user(admin, "hune_bad_insert", badPassword, [READ_A, { role: "hune_bad_insert_role", db: "admin" }], SHA256);
user(admin, "hune_bad_anyres", badPassword, [READ_A, { role: "hune_bad_anyres_role", db: "admin" }], SHA256);
user(admin, "hune_bad_cluster", badPassword, [READ_A, { role: "hune_bad_cluster_role", db: "admin" }], SHA256);
user(other, "hune_bad_authdb", badPassword, [READ_A], SHA256);

// The marker is the last write. The health check of the compose project reads it.
analytics.fixture_marker.insertOne({ _id: 1, node_load_docs: nodeLoadDocs, big_bytes: bigBytes });
print("init.js: done, node_load " + nodeLoadDocs + " documents");
