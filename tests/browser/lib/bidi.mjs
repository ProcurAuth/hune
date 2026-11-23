// Minimal WebDriver BiDi client for the browser suite (16.10), for Firefox.
// It has the same classes and methods as `cdp.mjs` (Browser, Context, Page) for the subset that the cases
// B01 to B18, B103, B112, B122, B141, B163 and B170 to B172 use. The runner loads this file when
// the variable HUNE_BROWSER is "firefox". The Chrome path (`cdp.mjs`) does not use this file.
// Firefox removed the Chrome DevTools Protocol, so each CDP call that a case makes directly through
// `page.send(...)` is mapped here to a BiDi command, or it throws an error that starts with
// "unsupported on Firefox:" (the report counts these as "not supported").
// Node 22 has a global WebSocket: no dependency is needed.
import { spawn } from "node:child_process";
import { appendFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
export { sleep };

const FIREFOX = process.env.FIREFOX || "/usr/bin/firefox-esr";
// Debug: HUNE_BIDI_TRACE=<file> writes each BiDi message (cut to 300 characters) with the time. It is off by default.
const TRACE = process.env.HUNE_BIDI_TRACE || null;
const trace = (dir, text) => {
  if (TRACE) appendFileSync(TRACE, `${new Date().toISOString()} ${dir} ${text.slice(0, 300)}\n`);
};

/** An error for a feature that Firefox or this transport does not give. */
export class Unsupported extends Error {
  constructor(what) {
    super(`unsupported on Firefox: ${what}`);
  }
}

/** Preferences of the disposable profile: no update, no telemetry, no first-run page. */
const PREFS = {
  "app.update.enabled": false,
  "app.update.auto": false,
  "browser.shell.checkDefaultBrowser": false,
  "browser.startup.page": 0,
  "browser.startup.homepage": "about:blank",
  "browser.newtabpage.enabled": false,
  "browser.aboutwelcome.enabled": false,
  "datareporting.policy.dataSubmissionEnabled": false,
  "datareporting.healthreport.uploadEnabled": false,
  "toolkit.telemetry.enabled": false,
  "toolkit.telemetry.unified": false,
  "browser.tabs.warnOnClose": false,
  "browser.sessionstore.resume_from_crash": false,
  "network.captive-portal-service.enabled": false,
  "network.connectivity-service.enabled": false,
  "extensions.update.enabled": false,
  "extensions.autoDisableScopes": 15,
  "signon.rememberSignons": false,
  "services.settings.server": "data:,#remote-settings-dummy/v1",
  "security.remote_settings.intermediates.enabled": false,
  "browser.safebrowsing.malware.enabled": false,
  "browser.safebrowsing.phishing.enabled": false,
  "app.normandy.enabled": false,
  "messaging-system.rsexperimentloader.enabled": false,
  "dom.events.asyncClipboard.readText": true,
  "dom.events.testing.asyncClipboard": true,
  "spellchecker.dictionary": "",
  "layout.spellcheckDefault": 0,
};

/** The key names of the cases mapped to the WebDriver key values. */
const KEYS = {
  Enter: "",
  Escape: "",
  Tab: "",
  Delete: "",
  Backspace: "",
  ArrowDown: "",
  ArrowUp: "",
  ArrowLeft: "",
  ArrowRight: "",
  Home: "",
  End: "",
  PageUp: "",
  PageDown: "",
  Shift: "",
  Control: "",
  Alt: "",
  Meta: "",
  " ": " ",
};

/** The modifier bits of CDP (Alt 1, Ctrl 2, Meta 4, Shift 8) as WebDriver key values. */
function modifierKeys(mask) {
  const out = [];
  if (mask & 1) out.push(KEYS.Alt);
  if (mask & 2) out.push(KEYS.Control);
  if (mask & 4) out.push(KEYS.Meta);
  if (mask & 8) out.push(KEYS.Shift);
  return out;
}

/** Turn a BiDi remote value into a JavaScript value, as `returnByValue` of CDP does. */
export function deserialize(v) {
  if (v === undefined || v === null) return undefined;
  switch (v.type) {
    case "undefined":
      return undefined;
    case "null":
      return null;
    case "string":
    case "boolean":
      return v.value;
    case "number":
      if (v.value === "NaN") return NaN;
      if (v.value === "Infinity") return Infinity;
      if (v.value === "-Infinity") return -Infinity;
      if (v.value === "-0") return -0;
      return v.value;
    case "bigint":
      return BigInt(v.value);
    case "array":
    case "set":
      return (v.value || []).map(deserialize);
    case "object": {
      const o = {};
      for (const [k, val] of v.value || []) o[typeof k === "string" ? k : String(deserialize(k))] = deserialize(val);
      return o;
    }
    case "map": {
      const o = {};
      for (const [k, val] of v.value || []) o[typeof k === "string" ? k : String(deserialize(k))] = deserialize(val);
      return o;
    }
    case "date":
      return v.value;
    case "regexp":
      return `/${v.value.pattern}/${v.value.flags || ""}`;
    default:
      return null;
  }
}

/** Start Firefox with the BiDi endpoint and connect to it. */
export class Browser {
  static async launch({ profileDir, width = 1400, height = 1000, args = [] }) {
    rmSync(profileDir, { recursive: true, force: true });
    mkdirSync(profileDir, { recursive: true });
    writeFileSync(
      join(profileDir, "user.js"),
      Object.entries(PREFS).map(([k, v]) => `user_pref(${JSON.stringify(k)}, ${JSON.stringify(v)});`).join("\n") + "\n",
    );
    const proc = spawn(
      FIREFOX,
      ["--headless", "--no-remote", "--profile", profileDir, "--remote-debugging-port", "0", "--width", String(width), "--height", String(height), ...args, "about:blank"],
      { stdio: ["ignore", "pipe", "pipe"], detached: true, env: { ...process.env, MOZ_HEADLESS: "1", MOZ_CRASHREPORTER_DISABLE: "1" } },
    );
    const b = new Browser(proc, width, height);
    let wsUrl = null;
    const scan = (chunk) => {
      const m = /WebDriver BiDi listening on (ws:\/\/\S+)/.exec(chunk.toString());
      if (m) wsUrl = m[1];
    };
    proc.stdout.on("data", scan);
    proc.stderr.on("data", scan);
    proc.on("exit", () => {
      b.exited = true;
    });
    for (let i = 0; i < 300 && !wsUrl && !b.exited; i++) await sleep(100);
    if (!wsUrl) {
      b.kill();
      throw new Error("Firefox did not publish the WebDriver BiDi endpoint");
    }
    await b.connect(wsUrl + "/session");
    return b;
  }

  constructor(proc, width, height) {
    this.proc = proc;
    this.width = width;
    this.height = height;
    this.nextId = 0;
    this.pending = new Map();
    this.pages = new Map(); // BiDi browsing context id -> Page
    this.exited = false;
    this.engine = "firefox";
  }

  async connect(url) {
    this.ws = new WebSocket(url);
    await new Promise((resolve, reject) => {
      this.ws.onopen = resolve;
      this.ws.onerror = () => reject(new Error("BiDi WebSocket error"));
    });
    this.ws.onmessage = (m) => {
      trace("<", String(m.data));
      this.#onMessage(JSON.parse(m.data));
    };
    const r = await this.send("session.new", { capabilities: { alwaysMatch: {} } });
    this.version = r.capabilities?.browserVersion;
    // The bodies of the requests (CDP gives them as `postData`).
    await this.send("network.addDataCollector", { dataTypes: ["request"], maxEncodedDataSize: 16 * 1024 * 1024 });
    await this.send("session.subscribe", {
      events: [
        "log.entryAdded",
        "network.beforeRequestSent",
        "network.responseCompleted",
        "network.fetchError",
        "browsingContext.userPromptOpened",
        "browsingContext.contextCreated",
      ],
    });
  }

  #onMessage(msg) {
    if (msg.type === "event") {
      const c = msg.params?.context ?? msg.params?.source?.context ?? msg.params?.request?.context;
      const page = c ? this.pages.get(c) : null;
      if (page) page._event(msg.method, msg.params);
      else if (msg.method === "browsingContext.contextCreated") this.#orphan(msg.params);
      return;
    }
    const p = this.pending.get(msg.id);
    if (!p) return;
    this.pending.delete(msg.id);
    if (msg.type === "error") p.reject(new Error(`${p.method}: ${msg.error}: ${msg.message}`));
    else p.resolve(msg.result);
  }

  /** A browsing context that the page opened (a popup): the parent page records it. */
  #orphan(p) {
    const opener = p.originalOpener ? this.pages.get(p.originalOpener) : null;
    if (opener) opener.popups.push(p);
  }

  /** The CDP calls that the cases make on the browser, mapped to BiDi. */
  async #cdp(method, params) {
    switch (method) {
      case "Browser.grantPermissions": {
        const names = { clipboardReadWrite: ["clipboard-read", "clipboard-write"], clipboardSanitizedWrite: ["clipboard-write"] };
        const wanted = new Set((params.permissions || []).flatMap((x) => names[x] || []));
        for (const name of wanted) {
          // Firefox 153 has no permission name "clipboard-read". The profile prefs `dom.events.testing.asyncClipboard` and
          // `dom.events.asyncClipboard.readText` let the page use the clipboard with no prompt, so a refusal is not an error.
          await this.send("permissions.setPermission", { descriptor: { name }, state: "granted", origin: params.origin, userContext: params.browserContextId }).catch(() => {});
        }
        return {};
      }
      default:
        throw new Unsupported(`CDP method ${method}`);
    }
  }

  send(method, params = {}) {
    if (/^[A-Z]/.test(method)) return this.#cdp(method, params);
    if (this.exited) return Promise.reject(new Error(`${method}: Firefox exited`));
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      // A command that Firefox never answers must not stop the runner: it fails after 60 s.
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`${method}: no answer from Firefox in 60 s`));
      }, 60000);
      this.pending.set(id, { resolve: (v) => (clearTimeout(timer), resolve(v)), reject: (e) => (clearTimeout(timer), reject(e)), method });
      const text = JSON.stringify({ id, method, params });
      trace(">", text);
      this.ws.send(text);
    });
  }

  /** A user context has its own cookies and localStorage. */
  async newContext(downloadDir = null) {
    const { userContext } = await this.send("browser.createUserContext", {});
    if (downloadDir) {
      mkdirSync(downloadDir, { recursive: true });
      try {
        await this.send("browser.setDownloadBehavior", { downloadBehavior: { type: "allowed", destinationFolder: downloadDir }, userContexts: [userContext] });
      } catch {
        // Firefox 153 may not accept it: a case that reads a download then fails by itself.
      }
    }
    return new Context(this, userContext);
  }

  kill() {
    try {
      process.kill(-this.proc.pid, "SIGKILL");
    } catch {
      try {
        this.proc.kill("SIGKILL");
      } catch {
        // already gone
      }
    }
  }

  async close() {
    try {
      this.ws?.close();
    } catch {
      // ignore
    }
    this.kill();
  }
}

export class Context {
  constructor(browser, id) {
    this.browser = browser;
    this.id = id;
    this.pages = [];
  }

  async newPage(opts = {}) {
    const b = this.browser;
    const { context } = await b.send("browsingContext.create", { type: "tab", userContext: this.id });
    const page = new Page(b, context, this);
    b.pages.set(context, page);
    this.pages.push(page);
    await page._init(opts);
    return page;
  }

  async dispose() {
    for (const p of this.pages) this.browser.pages.delete(p.contextId);
    try {
      await this.browser.send("browser.removeUserContext", { userContext: this.id });
    } catch {
      // ignore
    }
  }
}

/** One page: evaluate, click, type, wait, record console problems and requests. */
export class Page {
  constructor(browser, contextId, context) {
    this.browser = browser;
    this.contextId = contextId;
    this.context = context;
    this.problems = []; // console errors, exceptions, CSP violations
    this.requests = []; // {id, method, url, headers, body, status}
    this.byId = new Map();
    this.pausedHandlers = [];
    this.dialogs = [];
    this.popups = [];
    this.allow = [];
    this.extraHeaders = {};
    this.ready = Promise.resolve(); // the intercept command, when a rule asked for it
    this.interceptSet = false;
    this.pointer = { x: 0, y: 0 };
    this.engine = "firefox";
  }

  /** The CDP calls that the cases make directly, mapped to BiDi. */
  async send(method, params = {}) {
    switch (method) {
      case "Emulation.setDeviceMetricsOverride":
        return this.browser.send("browsingContext.setViewport", {
          context: this.contextId,
          viewport: { width: params.width, height: params.height },
          devicePixelRatio: params.deviceScaleFactor || 1,
        });
      case "Page.addScriptToEvaluateOnNewDocument":
        return this.browser.send("script.addPreloadScript", { functionDeclaration: `() => { ${params.source} }`, contexts: [this.contextId] });
      case "Page.captureScreenshot": {
        const p = { context: this.contextId, origin: "viewport" };
        if (params.clip) p.clip = { type: "box", x: params.clip.x, y: params.clip.y, width: params.clip.width, height: params.clip.height };
        await this.activate();
        const r = await this.browser.send("browsingContext.captureScreenshot", p);
        return { data: r.data };
      }
      case "Input.insertText":
        return this.#typeText(params.text);
      case "Input.dispatchMouseEvent":
        return this.#mouse(params);
      case "Input.dispatchKeyEvent":
        return this.#keyEvent(params);
      case "Network.setExtraHTTPHeaders":
        return this.setHeaders(params.headers);
      case "Emulation.setTimezoneOverride":
        return this.browser.send("emulation.setTimezoneOverride", { timezone: params.timezoneId, contexts: [this.contextId] });
      case "Emulation.setEmulatedMedia": {
        const feats = params.features || [];
        const unsupported = [];
        if (params.media) unsupported.push(`media ${params.media}`);
        for (const f of feats) {
          if (f.name === "prefers-color-scheme") await this._setColorScheme(f.value);
          else unsupported.push(f.name);
        }
        if (unsupported.length) throw new Unsupported(`Emulation.setEmulatedMedia (${unsupported.join(", ")}): WebDriver BiDi in Firefox 153 has no command for a media type or a media feature`);
        return {};
      }
      case "Input.imeSetComposition":
        throw new Unsupported("Input.imeSetComposition: WebDriver BiDi has no IME command");
      case "Accessibility.enable":
      case "Accessibility.getFullAXTree":
        throw new Unsupported("Accessibility tree: WebDriver BiDi has no accessibility command");
      case "Profiler.enable":
      case "Profiler.setSamplingInterval":
      case "Profiler.start":
      case "Profiler.stop":
      case "Profiler.disable":
        throw new Unsupported("Profiler: WebDriver BiDi has no profiler command");
      default:
        throw new Unsupported(`CDP method ${method}`);
    }
  }

  async _init({ width, height } = {}) {
    await this.browser.send("browsingContext.setViewport", {
      context: this.contextId,
      viewport: { width: width || this.browser.width, height: height || this.browser.height },
      devicePixelRatio: 1,
    });
    // Record CSP violations that the page raises (9.29, B15).
    await this.browser.send("script.addPreloadScript", {
      functionDeclaration: `() => { document.addEventListener("securitypolicyviolation", (e) => {
        console.error("CSP violation: " + e.violatedDirective + " " + e.blockedURI);
      }); }`,
      contexts: [this.contextId],
    });
  }

  /**
   * The CDP client pauses each request from the start (`Fetch.enable`). Firefox 153 stopped to answer on its BiDi socket
   * from time to time while it held many paused requests, so this client pauses the requests of a page only after the
   * page got a rule (`failWhen`, `fulfillWhen`, `setHeaders`). The call of a page waits until the intercept is set.
   */
  #needIntercept() {
    if (this.interceptSet) return;
    this.interceptSet = true;
    this.ready = this.ready.then(() => this.browser.send("network.addIntercept", { phases: ["beforeRequestSent"], contexts: [this.contextId] })).then(() => {}, () => {});
  }

  _event(method, p) {
    switch (method) {
      case "log.entryAdded":
        if (p.level === "error") {
          if (p.type === "javascript") this.problems.push("exception: " + p.text);
          else this.problems.push("console.error: " + (p.args && p.args.every((a) => a.type === "string") ? p.args.map((a) => a.value).join(" ") : p.text || p.args.map((a) => this.#show(a)).join(" ")));
        }
        break;
      case "network.beforeRequestSent": {
        const headers = {};
        for (const h of p.request.headers || []) headers[h.name] = h.value?.value ?? "";
        const r = { id: p.request.request, method: p.request.method, url: p.request.url, headers, body: null, status: null, failed: null, t: Date.now() };
        if (!this.byId.has(r.id)) {
          this.byId.set(r.id, r);
          this.requests.push(r);
        }
        if (p.isBlocked) this.#paused(p, headers);
        if (p.request.bodySize > 0) this.#fetchBody(r);
        break;
      }
      case "network.responseCompleted": {
        const r = this.byId.get(p.request.request);
        if (r) {
          r.status = p.response.status;
          r.responseHeaders = Object.fromEntries((p.response.headers || []).map((h) => [h.name.toLowerCase(), h.value?.value ?? ""]));
        }
        break;
      }
      case "network.fetchError": {
        const r = this.byId.get(p.request.request);
        if (r) r.failed = p.errorText;
        break;
      }
      case "browsingContext.userPromptOpened":
        this.dialogs.push(p);
        this.browser.send("browsingContext.handleUserPrompt", { context: this.contextId, accept: true }).catch(() => {});
        break;
      default:
        break;
    }
  }

  /** Read the body of a request after the request was sent (a few tries: the data appears at send time). */
  async #fetchBody(r) {
    for (let i = 0; i < 20 && r.body === null; i++) {
      try {
        const g = await this.browser.send("network.getData", { request: r.id, dataType: "request" });
        r.body = g.bytes.type === "base64" ? Buffer.from(g.bytes.value, "base64").toString("utf8") : g.bytes.value;
        return;
      } catch {
        await sleep(30);
      }
    }
  }

  /** A console argument as text: a string as it is, an object as JSON. */
  #show(a) {
    const v = deserialize(a);
    if (typeof v === "string") return v;
    try {
      return JSON.stringify(v) ?? String(v);
    } catch {
      return String(v);
    }
  }

  #paused(p, headers) {
    const id = p.request.request;
    const req = { url: p.request.url, method: p.request.method, headers };
    for (const h of this.pausedHandlers) {
      const r = h(req);
      if (r === "fail") {
        this.browser.send("network.failRequest", { request: id }).catch(() => {});
        return;
      }
      if (r && typeof r === "object" && r.fulfill) {
        const hs = Object.entries(r.fulfill.headers || {}).map(([name, value]) => ({ name, value: { type: "string", value: String(value) } }));
        this.browser
          .send("network.provideResponse", {
            request: id,
            statusCode: r.fulfill.status,
            headers: hs,
            body: { type: "base64", value: Buffer.from(r.fulfill.body || "").toString("base64") },
          })
          .catch(() => {});
        return;
      }
    }
    const extra = Object.entries(this.extraHeaders);
    if (extra.length) {
      const all = { ...headers, ...this.extraHeaders };
      const hs = Object.entries(all).map(([name, value]) => ({ name, value: { type: "string", value: String(value) } }));
      this.browser.send("network.continueRequest", { request: id, headers: hs }).catch(() => {});
    } else {
      this.browser.send("network.continueRequest", { request: id }).catch(() => {});
    }
  }

  /** Fail each request for which `pred(request)` is true. Returns a function that removes the rule. */
  failWhen(pred) {
    const h = (req) => (pred(req) ? "fail" : null);
    this.pausedHandlers.push(h);
    this.#needIntercept();
    return () => {
      this.pausedHandlers = this.pausedHandlers.filter((x) => x !== h);
    };
  }

  /** Answer each request for which `pred(request)` is true with a fixed response. */
  fulfillWhen(pred, fulfill) {
    const h = (req) => (pred(req) ? { fulfill } : null);
    this.pausedHandlers.push(h);
    this.#needIntercept();
    return () => {
      this.pausedHandlers = this.pausedHandlers.filter((x) => x !== h);
    };
  }

  /** Send these headers with each request of the page. `{}` removes them. */
  async setHeaders(headers) {
    this.extraHeaders = headers || {};
    if (Object.keys(this.extraHeaders).length) this.#needIntercept();
    await this.ready;
  }

  /**
   * Chrome headless shows every page. Firefox shows the selected tab of the window only, and the other tabs are hidden
   * (`document.hidden`, throttled timers). Select this page before a call that needs a visible page.
   */
  async activate() {
    await this.ready;
    if (this.browser.active === this.contextId) return;
    this.browser.active = this.contextId;
    await this.browser.send("browsingContext.activate", { context: this.contextId }).catch(() => {});
  }

  async eval(expression, { awaitPromise = true } = {}) {
    await this.activate();
    const r = await this.browser.send("script.evaluate", {
      expression,
      target: { context: this.contextId },
      awaitPromise,
      resultOwnership: "none",
      userActivation: true,
    });
    if (r.type === "exception") {
      const d = r.exceptionDetails;
      throw new Error("eval: " + (d?.text || "exception"));
    }
    return deserialize(r.result);
  }

  async goto(url) {
    await this.ready;
    await this.browser.send("browsingContext.navigate", { context: this.contextId, url, wait: "interactive" });
    await this.waitFor("document.readyState !== 'loading' && !!document.getElementById('app-main')", { what: "shell load" });
  }

  async reload() {
    await this.ready;
    await this.browser.send("browsingContext.reload", { context: this.contextId, wait: "interactive" });
    await sleep(100);
    await this.waitFor("document.readyState !== 'loading' && !!document.getElementById('app-main')", { what: "shell reload" });
  }

  /** Wait until a JavaScript expression is truthy. */
  async waitFor(expr, { timeout = 15000, what = expr } = {}) {
    const end = Date.now() + timeout;
    let last;
    for (;;) {
      try {
        const v = await this.eval(expr);
        if (v) return v;
      } catch (e) {
        last = e;
      }
      if (Date.now() > end) throw new Error(`timeout waiting for ${what}${last ? " (" + last.message + ")" : ""}`);
      await sleep(40);
    }
  }

  /** Wait for `data-hune-state="idle"`, stable over two reads (9.29). */
  async idle({ timeout = 20000 } = {}) {
    const end = Date.now() + timeout;
    let stable = 0;
    while (Date.now() < end) {
      const s = await this.eval("document.documentElement.getAttribute('data-hune-state')").catch(() => null);
      if (s === "idle") {
        if (++stable >= 2) return;
      } else {
        stable = 0;
      }
      await sleep(60);
    }
    throw new Error("timeout waiting for data-hune-state=idle");
  }

  q(sel) {
    return `document.querySelector(${JSON.stringify(sel)})`;
  }

  tid(id) {
    return `[data-testid=${JSON.stringify(id)}]`;
  }

  async exists(sel) {
    return this.eval(`!!${this.q(sel)}`);
  }

  async count(sel) {
    return this.eval(`document.querySelectorAll(${JSON.stringify(sel)}).length`);
  }

  async text(sel) {
    return this.eval(`(${this.q(sel)}||{}).textContent ?? null`);
  }

  async attr(sel, name) {
    return this.eval(`(${this.q(sel)}||{getAttribute(){return null}}).getAttribute(${JSON.stringify(name)})`);
  }

  async waitSel(sel, opts = {}) {
    return this.waitFor(`!!${this.q(sel)}`, { what: `selector ${sel}`, ...opts });
  }

  async waitGone(sel, opts = {}) {
    return this.waitFor(`!${this.q(sel)}`, { what: `no selector ${sel}`, ...opts });
  }

  /** The centre of an element in the viewport, after a scroll into view. */
  async #centre(sel) {
    await this.waitSel(sel);
    return this.eval(`(() => {
      const e = ${this.q(sel)};
      e.scrollIntoView({block: "center", inline: "center"});
      const b = e.getBoundingClientRect();
      return {x: b.left + b.width / 2, y: b.top + b.height / 2, w: b.width, h: b.height, disabled: !!e.disabled};
    })()`);
  }

  async #actions(context, sources) {
    await this.activate();
    await this.browser.send("input.performActions", { context, actions: sources });
  }

  /** One CDP mouse event as one BiDi pointer action. */
  async #mouse({ type, x, y, button = "left", clickCount = 1, modifiers = 0 }) {
    const mods = modifierKeys(modifiers);
    if (mods.length) await this.#actions(this.contextId, [{ type: "key", id: "kbd-mod", actions: mods.map((value) => ({ type: "keyDown", value })) }]);
    const move = { type: "pointerMove", x: Math.round(x), y: Math.round(y), origin: "viewport" };
    const b = { left: 0, middle: 1, right: 2 }[button] ?? 0;
    const acts = [move];
    if (type === "mousePressed") for (let i = 0; i < Math.max(1, clickCount); i++) acts.push({ type: "pointerDown", button: b }, ...(i < clickCount - 1 ? [{ type: "pointerUp", button: b }] : []));
    if (type === "mouseReleased") acts.push({ type: "pointerUp", button: b });
    await this.#actions(this.contextId, [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: acts }]);
    if (mods.length) await this.#actions(this.contextId, [{ type: "key", id: "kbd-mod", actions: mods.map((value) => ({ type: "keyUp", value })) }]);
  }

  /** A real mouse click at the centre of the element. */
  async click(sel, { modifiers = 0, clickCount = 1 } = {}) {
    const { x, y, w, h } = await this.#centre(sel);
    if (!w || !h) throw new Error(`element ${sel} has no size`);
    const mods = modifierKeys(modifiers);
    if (mods.length) await this.#actions(this.contextId, [{ type: "key", id: "kbd-mod", actions: mods.map((value) => ({ type: "keyDown", value })) }]);
    const acts = [{ type: "pointerMove", x: Math.round(x), y: Math.round(y), origin: "viewport" }];
    for (let i = 0; i < clickCount; i++) acts.push({ type: "pointerDown", button: 0 }, { type: "pointerUp", button: 0 });
    await this.#actions(this.contextId, [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: acts }]);
    if (mods.length) await this.#actions(this.contextId, [{ type: "key", id: "kbd-mod", actions: mods.map((value) => ({ type: "keyUp", value })) }]);
  }

  /** Click the first element that matches `sel` and whose text is `text` (exact after trim). */
  async clickText(sel, text) {
    const mark = `data-b-click-${Math.random().toString(36).slice(2)}`;
    const ok = await this.eval(`(() => {
      for (const e of document.querySelectorAll(${JSON.stringify(sel)})) {
        if (e.textContent.trim() === ${JSON.stringify(text)}) { e.setAttribute(${JSON.stringify(mark)}, "1"); return true; }
      }
      return false;
    })()`);
    if (!ok) throw new Error(`no ${sel} with text "${text}"`);
    await this.click(`[${mark}]`);
    await this.eval(`document.querySelector('[${mark}]')?.removeAttribute(${JSON.stringify(mark)})`);
  }

  async focus(sel) {
    await this.waitSel(sel);
    await this.eval(`${this.q(sel)}.focus()`);
  }

  /**
   * Insert a text as `Input.insertText` of CDP does: one `beforeinput` and one `input` event, one undo step. WebDriver has
   * no such command, so the page runs `document.execCommand("insertText")` on the focused element. When Firefox refuses
   * it (it returns false), the text goes in with real key events, one key down and one key up for each character.
   */
  async #typeText(text) {
    const done = await this.eval(`document.execCommand("insertText", false, ${JSON.stringify(text)})`).catch(() => false);
    if (done === true) return;
    const chars = Array.from(text);
    for (let i = 0; i < chars.length; i += 400) {
      const actions = [];
      for (const c of chars.slice(i, i + 400)) {
        const value = c === "\n" ? KEYS.Enter : c === "\t" ? KEYS.Tab : c;
        actions.push({ type: "keyDown", value }, { type: "keyUp", value });
      }
      await this.#actions(this.contextId, [{ type: "key", id: "kbd", actions }]);
    }
  }

  /**
   * One CDP key event as BiDi key actions. A key down presses the modifiers that are not down yet, and then the key.
   * A key up releases the key, and then the modifiers. `text`, `commands` and `autoRepeat` of CDP have no BiDi form.
   */
  async #keyEvent({ type, key, modifiers = 0 }) {
    const value = KEYS[key] ?? (key.length === 1 ? key : null);
    if (!value) throw new Unsupported(`key "${key}" (a dead key or a named key that WebDriver does not define)`);
    const mods = modifierKeys(modifiers);
    this.held ??= new Set();
    const acts = [];
    if (type === "keyDown" || type === "rawKeyDown") {
      for (const m of mods) if (!this.held.has(m)) (this.held.add(m), acts.push({ type: "keyDown", value: m }));
      acts.push({ type: "keyDown", value });
    } else {
      acts.push({ type: "keyUp", value });
      for (const m of [...mods].reverse()) if (this.held.has(m)) (this.held.delete(m), acts.push({ type: "keyUp", value: m }));
    }
    await this.#actions(this.contextId, [{ type: "key", id: "kbd", actions: acts }]);
  }

  /** Replace the value of a text field with real input events. */
  async fill(sel, text) {
    await this.focus(sel);
    await this.eval(`(() => { const e = ${this.q(sel)}; if (e.select) e.select(); })()`);
    if (text === "") {
      await this.press("Delete");
    } else {
      await this.#typeText(text);
    }
  }

  async type(sel, text) {
    await this.focus(sel);
    await this.#typeText(text);
  }

  async press(key, { modifiers = 0 } = {}) {
    const value = KEYS[key] ?? key;
    const mods = modifierKeys(modifiers);
    const actions = [
      ...mods.map((m) => ({ type: "keyDown", value: m })),
      { type: "keyDown", value },
      { type: "keyUp", value },
      ...mods.reverse().map((m) => ({ type: "keyUp", value: m })),
    ];
    await this.#actions(this.contextId, [{ type: "key", id: "kbd", actions }]);
  }

  /** Select an option of a `<select>` by value, and fire `input` and `change`. */
  async select(sel, value) {
    await this.waitSel(sel);
    await this.eval(`(() => {
      const e = ${this.q(sel)};
      e.value = ${JSON.stringify(value)};
      e.dispatchEvent(new Event("input", {bubbles: true}));
      e.dispatchEvent(new Event("change", {bubbles: true}));
    })()`);
  }

  /** Make `document.hidden` true or false and fire `visibilitychange` (headless has no hidden tab). */
  async setHidden(hidden) {
    await this.eval(`(() => {
      Object.defineProperty(document, "hidden", {configurable: true, get: () => ${hidden}});
      Object.defineProperty(document, "visibilityState", {configurable: true, get: () => ${hidden ? '"hidden"' : '"visible"'}});
      document.dispatchEvent(new Event("visibilitychange"));
    })()`);
  }

  /** Run the page in another time zone (an IANA name). Call it before `goto`. */
  async emulateTimezone(timezoneId) {
    await this.send("Emulation.setTimezoneOverride", { timezoneId });
  }

  /**
   * Record the texts that the page draws on canvases (axis labels), for the next documents of this page.
   * Read them with `canvasTexts`. Call it before `goto`.
   */
  async recordCanvasText() {
    await this.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `(() => {
        const rec = new WeakMap();
        const proto = CanvasRenderingContext2D.prototype;
        const fill = proto.fillText;
        const clear = proto.clearRect;
        proto.fillText = function (text, x, y, ...rest) {
          let list = rec.get(this.canvas);
          if (!list) rec.set(this.canvas, (list = []));
          list.push({ text: String(text), x, y, align: this.textAlign, baseline: this.textBaseline });
          return fill.call(this, text, x, y, ...rest);
        };
        proto.clearRect = function (x, y, w, h) {
          if (x === 0 && y === 0) rec.set(this.canvas, []);
          return clear.call(this, x, y, w, h);
        };
        window.__canvasTexts = (canvas) => rec.get(canvas) || [];
      })();`,
    });
  }

  /** Move the mouse over an element. `fx` and `fy` are fractions of its box. */
  async hover(sel, fx = 0.5, fy = 0.5) {
    await this.waitSel(sel);
    const r = await this.eval(`(() => {
      const e = ${this.q(sel)};
      e.scrollIntoView({block: "center", inline: "center"});
      const b = e.getBoundingClientRect();
      return {x: b.left + b.width * ${fx}, y: b.top + b.height * ${fy}};
    })()`);
    await this.#mouse({ type: "mouseMoved", x: r.x, y: r.y });
    await this.#mouse({ type: "mouseMoved", x: r.x + 1, y: r.y });
  }

  async emulateColorScheme(value) {
    await this.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value }] });
  }

  /**
   * Firefox has no media emulation in WebDriver BiDi. The page then gets the theme through its own control (9.25):
   * the key "hune.theme" of localStorage and the event "hune-theme". This tests the two themes. It does not test
   * the media query. The key is set for the next documents (preload script) and for the document that is open.
   */
  async _setColorScheme(value) {
    if (this.schemeScript) await this.browser.send("script.removePreloadScript", { script: this.schemeScript }).catch(() => {});
    const set = `try { localStorage.setItem("hune.theme", ${JSON.stringify(value)}); } catch (e) {}`;
    const r = await this.browser.send("script.addPreloadScript", { functionDeclaration: `() => { ${set} }`, contexts: [this.contextId] });
    this.schemeScript = r.script;
    await this.eval(`(() => { ${set} document.dispatchEvent(new Event("hune-theme")); })()`).catch(() => {});
  }

  async screenshot(path) {
    const r = await this.send("Page.captureScreenshot", {});
    writeFileSync(path, Buffer.from(r.data, "base64"));
  }

  /** The requests whose URL path ends with / contains `part`. */
  reqs(part, method) {
    return this.requests.filter((r) => r.url.includes(part) && (!method || r.method === method));
  }

  clearRequests() {
    this.requests.length = 0;
  }

  async close() {
    try {
      await this.browser.send("browsingContext.close", { context: this.contextId });
    } catch {
      // ignore
    }
    this.browser.pages.delete(this.contextId);
  }

  /** Console problems that no `allow` pattern covers. */
  unexpectedProblems() {
    return this.problems.filter((p) => !this.allow.some((re) => re.test(p)));
  }
}
