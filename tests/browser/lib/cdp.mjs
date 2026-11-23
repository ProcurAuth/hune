// SPDX-License-Identifier: MIT OR Apache-2.0
// Minimal Chrome DevTools Protocol client for the browser suite (16.10).
// One browser process, one WebSocket, one session for each page (flat mode).
// Node 22 has a global WebSocket and fetch: no dependency is needed.
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
export { sleep };

const CHROME = process.env.CHROME || "/usr/bin/google-chrome";

/** Start Chrome and connect to it. */
export class Browser {
  static async launch({ profileDir, width = 1400, height = 1000, args = [] }) {
    rmSync(profileDir, { recursive: true, force: true });
    mkdirSync(profileDir, { recursive: true });
    const proc = spawn(
      CHROME,
      [
        "--headless=new",
        "--remote-debugging-port=0",
        `--user-data-dir=${profileDir}`,
        "--no-sandbox",
        "--disable-gpu",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-extensions",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-sync",
        "--metrics-recording-only",
        `--window-size=${width},${height}`,
        ...args,
        "about:blank",
      ],
      { stdio: "ignore", detached: true },
    );
    const b = new Browser(proc, width, height);
    const file = join(profileDir, "DevToolsActivePort");
    let wsUrl = null;
    for (let i = 0; i < 150 && !wsUrl; i++) {
      try {
        const [port, path] = readFileSync(file, "utf8").trim().split("\n");
        if (port && path) wsUrl = `ws://127.0.0.1:${port}${path}`;
      } catch {
        // not written yet
      }
      if (!wsUrl) await sleep(100);
    }
    if (!wsUrl) {
      b.kill();
      throw new Error("Chrome did not publish DevToolsActivePort");
    }
    await b.connect(wsUrl);
    return b;
  }

  constructor(proc, width, height) {
    this.proc = proc;
    this.width = width;
    this.height = height;
    this.nextId = 0;
    this.pending = new Map();
    this.sessions = new Map(); // sessionId -> Page
  }

  async connect(url) {
    this.ws = new WebSocket(url);
    await new Promise((resolve, reject) => {
      this.ws.onopen = resolve;
      this.ws.onerror = () => reject(new Error("DevTools WebSocket error"));
    });
    this.ws.onmessage = (m) => this.#onMessage(JSON.parse(m.data));
  }

  #onMessage(msg) {
    if (msg.id !== undefined) {
      const p = this.pending.get(msg.id);
      if (p) {
        this.pending.delete(msg.id);
        if (msg.error) p.reject(new Error(`${p.method}: ${msg.error.message}`));
        else p.resolve(msg.result);
      }
      return;
    }
    const page = msg.sessionId ? this.sessions.get(msg.sessionId) : null;
    if (page) page._event(msg.method, msg.params);
  }

  send(method, params = {}, sessionId) {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject, method });
      const msg = { id, method, params };
      if (sessionId) msg.sessionId = sessionId;
      this.ws.send(JSON.stringify(msg));
    });
  }

  /** A browser context has its own cookies and localStorage. */
  async newContext(downloadDir = null) {
    const { browserContextId } = await this.send("Target.createBrowserContext", { disposeOnDetach: false });
    if (downloadDir) {
      mkdirSync(downloadDir, { recursive: true });
      await this.send("Browser.setDownloadBehavior", { behavior: "allow", downloadPath: downloadDir, browserContextId });
    }
    return new Context(this, browserContextId);
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
    const { targetId } = await b.send("Target.createTarget", { url: "about:blank", browserContextId: this.id });
    const { sessionId } = await b.send("Target.attachToTarget", { targetId, flatten: true });
    const page = new Page(b, sessionId, targetId, this);
    b.sessions.set(sessionId, page);
    this.pages.push(page);
    await page._init(opts);
    return page;
  }

  async dispose() {
    for (const p of this.pages) b_unregister(this.browser, p);
    try {
      await this.browser.send("Target.disposeBrowserContext", { browserContextId: this.id });
    } catch {
      // ignore
    }
  }
}

function b_unregister(browser, page) {
  browser.sessions.delete(page.sessionId);
}

/** One page: evaluate, click, type, wait, record console problems and requests. */
export class Page {
  constructor(browser, sessionId, targetId, context) {
    this.browser = browser;
    this.sessionId = sessionId;
    this.targetId = targetId;
    this.context = context;
    this.problems = []; // console errors, exceptions, CSP violations
    this.requests = []; // {id, method, url, headers, body, status}
    this.byId = new Map();
    this.waiters = [];
    this.pausedHandlers = [];
    this.dialogs = [];
    this.popups = [];
    this.allow = [];
  }

  send(method, params = {}) {
    return this.browser.send(method, params, this.sessionId);
  }

  async _init({ width, height } = {}) {
    await this.send("Page.enable");
    await this.send("Runtime.enable");
    await this.send("Log.enable");
    await this.send("Network.enable");
    await this.send("Emulation.setDeviceMetricsOverride", {
      width: width || this.browser.width,
      height: height || this.browser.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    // Record CSP violations that the page raises (9.29, B15).
    await this.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `document.addEventListener("securitypolicyviolation", (e) => {
        console.error("CSP violation: " + e.violatedDirective + " " + e.blockedURI);
      });`,
    });
    await this.send("Fetch.enable", { patterns: [{ urlPattern: "*", requestStage: "Request" }] });
  }

  _event(method, p) {
    switch (method) {
      case "Runtime.exceptionThrown": {
        const d = p.exceptionDetails;
        this.problems.push("exception: " + (d.exception?.description || d.text));
        break;
      }
      case "Runtime.consoleAPICalled":
        if (p.type === "error") {
          this.problems.push("console.error: " + p.args.map((a) => a.value ?? a.description ?? "").join(" "));
        }
        break;
      case "Log.entryAdded":
        if (p.entry.level === "error") this.problems.push(`log: ${p.entry.text} ${p.entry.url || ""}`);
        break;
      case "Network.requestWillBeSent": {
        const r = { id: p.requestId, method: p.request.method, url: p.request.url, headers: p.request.headers, body: p.request.postData ?? null, status: null, failed: null, t: Date.now() };
        this.byId.set(p.requestId, r);
        this.requests.push(r);
        break;
      }
      case "Network.requestWillBeSentExtraInfo": {
        const r = this.byId.get(p.requestId);
        if (r) r.headers = { ...r.headers, ...p.headers };
        break;
      }
      case "Network.responseReceived": {
        const r = this.byId.get(p.requestId);
        if (r) {
          r.status = p.response.status;
          r.responseHeaders = p.response.headers;
        }
        break;
      }
      case "Network.loadingFailed": {
        const r = this.byId.get(p.requestId);
        if (r) r.failed = p.errorText;
        break;
      }
      case "Fetch.requestPaused":
        this.#paused(p);
        break;
      case "Page.javascriptDialogOpening":
        this.dialogs.push(p);
        this.send("Page.handleJavaScriptDialog", { accept: true }).catch(() => {});
        break;
      default:
        break;
    }
  }

  #paused(p) {
    for (const h of this.pausedHandlers) {
      const r = h(p.request);
      if (r === "fail") {
        this.send("Fetch.failRequest", { requestId: p.requestId, errorReason: "ConnectionFailed" }).catch(() => {});
        return;
      }
      if (r && typeof r === "object" && r.fulfill) {
        this.send("Fetch.fulfillRequest", {
          requestId: p.requestId,
          responseCode: r.fulfill.status,
          responseHeaders: Object.entries(r.fulfill.headers || {}).map(([name, value]) => ({ name, value })),
          body: Buffer.from(r.fulfill.body || "").toString("base64"),
        }).catch(() => {});
        return;
      }
    }
    this.send("Fetch.continueRequest", { requestId: p.requestId }).catch(() => {});
  }

  /** Fail each request for which `pred(request)` is true. Returns a function that removes the rule. */
  failWhen(pred) {
    const h = (req) => (pred(req) ? "fail" : null);
    this.pausedHandlers.push(h);
    return () => {
      this.pausedHandlers = this.pausedHandlers.filter((x) => x !== h);
    };
  }

  /** Answer each request for which `pred(request)` is true with a fixed response. */
  fulfillWhen(pred, fulfill) {
    const h = (req) => (pred(req) ? { fulfill } : null);
    this.pausedHandlers.push(h);
    return () => {
      this.pausedHandlers = this.pausedHandlers.filter((x) => x !== h);
    };
  }

  /** Send these headers with each request of the page (the edge that adds the identity token is absent). `{}` removes them. */
  async setHeaders(headers) {
    await this.send("Network.setExtraHTTPHeaders", { headers });
  }

  async eval(expression, { awaitPromise = true } = {}) {
    const r = await this.send("Runtime.evaluate", { expression, awaitPromise, returnByValue: true, userGesture: true });
    if (r.exceptionDetails) {
      throw new Error("eval: " + (r.exceptionDetails.exception?.description || r.exceptionDetails.text));
    }
    return r.result.value;
  }

  async goto(url) {
    await this.send("Page.navigate", { url });
    await this.waitFor("document.readyState !== 'loading' && !!document.getElementById('app-main')", { what: "shell load" });
  }

  async reload() {
    await this.send("Page.reload", {});
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
    const r = await this.eval(`(() => {
      const e = ${this.q(sel)};
      e.scrollIntoView({block: "center", inline: "center"});
      const b = e.getBoundingClientRect();
      return {x: b.left + b.width / 2, y: b.top + b.height / 2, w: b.width, h: b.height, disabled: !!e.disabled};
    })()`);
    return r;
  }

  /** A real mouse click at the centre of the element. */
  async click(sel, { modifiers = 0, clickCount = 1 } = {}) {
    const { x, y, w, h } = await this.#centre(sel);
    if (!w || !h) throw new Error(`element ${sel} has no size`);
    await this.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await this.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount, modifiers });
    await this.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount, modifiers });
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

  /** Replace the value of a text field with real input events. */
  async fill(sel, text) {
    await this.focus(sel);
    await this.eval(`(() => { const e = ${this.q(sel)}; if (e.select) e.select(); })()`);
    if (text === "") {
      await this.press("Delete");
    } else {
      await this.send("Input.insertText", { text });
    }
  }

  async type(sel, text) {
    await this.focus(sel);
    await this.send("Input.insertText", { text });
  }

  async press(key, { modifiers = 0 } = {}) {
    const codes = { Enter: 13, Escape: 27, Tab: 9, Delete: 46, Backspace: 8, ArrowDown: 40, ArrowUp: 38, ArrowLeft: 37, ArrowRight: 39, " ": 32 };
    const vk = codes[key] ?? key.toUpperCase().charCodeAt(0);
    const text = key === "Enter" ? "\r" : key.length === 1 ? key : undefined;
    const base = { key, code: key.length === 1 ? "Key" + key.toUpperCase() : key, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
    await this.send("Input.dispatchKeyEvent", { type: text ? "keyDown" : "rawKeyDown", ...base, text });
    await this.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
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
    await this.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: r.x, y: r.y });
    await this.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: r.x + 1, y: r.y });
  }

  async emulateColorScheme(value) {
    await this.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value }] });
  }

  async screenshot(path) {
    const r = await this.send("Page.captureScreenshot", { format: "png" });
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
      await this.browser.send("Target.closeTarget", { targetId: this.targetId });
    } catch {
      // ignore
    }
    this.browser.sessions.delete(this.sessionId);
  }

  /** Console problems that no `allow` pattern covers. */
  unexpectedProblems() {
    return this.problems.filter((p) => !this.allow.some((re) => re.test(p)));
  }
}
