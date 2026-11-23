// Reference screenshots: static HTML pages of other products, rendered from the file system (file:// URLs).
// The harness only reads these files. It starts no server of these products and it blocks every request that is not
// a file or a data URL, so no network or production host is contacted.
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { capture, openPage, sleep, THEMES } from "./kit.mjs";

function withTimeout(promise, ms, what) {
  let timer;
  return Promise.race([promise, new Promise((_, rej) => (timer = setTimeout(() => rej(new Error(`timeout of ${ms} ms in ${what}`)), ms)))]).finally(() => clearTimeout(timer));
}

/**
 * [name, absolute path]. The test tooling variable HUNE_SHOTS_REFERENCES gives the list as "name=path;name=path".
 * The list is empty when the variable is not set. A page that does not exist is reported and skipped.
 */
export const REFERENCES = (process.env.HUNE_SHOTS_REFERENCES || "")
  .split(";")
  .filter(Boolean)
  .map((p) => [p.slice(0, p.indexOf("=")), p.slice(p.indexOf("=") + 1)]);

const WIDTHS = [1280, 390];

export async function captureReferences(browser, outDir) {
  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });
  const context = await browser.newContext();
  const done = [];
  const skipped = [];
  try {
    for (const [name, path] of REFERENCES) {
      if (!existsSync(path)) {
        skipped.push(`${name}: ${path} does not exist`);
        continue;
      }
      for (const theme of ["light", "dark"]) {
        for (const width of WIDTHS) {
          const page = await openPage(context, { theme, width });
          // Allow file and data URLs only.
          page.failWhen((r) => !/^(file|data|about):/.test(r.url));
          try {
            const file = `${name}-${theme}-${width}.png`;
            await withTimeout(
              (async () => {
                await page.send("Page.navigate", { url: "file://" + path });
                await page.waitFor("document.readyState === 'complete'", { timeout: 15000, what: "load" });
                await sleep(1200);
                await capture(page, join(outDir, file), { full: true, maxHeight: 6000, noSettle: true });
              })(),
              45000,
              `${name} ${theme} ${width}`,
            );
            done.push({ name, theme, width, file, source: path });
          } catch (e) {
            skipped.push(`${name} ${theme} ${width}: ${e.message}`);
          } finally {
            await page.close().catch(() => {});
          }
        }
      }
    }
  } finally {
    await context.dispose().catch(() => {});
  }
  writeFileSync(join(outDir, "manifest.json"), JSON.stringify({ done, skipped }, null, 2) + "\n");
  const rows = done.map((d) => `<figure><a href="${d.file}"><img loading="lazy" src="${d.file}" alt=""></a><figcaption>${d.name} · ${d.theme} · ${d.width}<br><small>${d.source}</small></figcaption></figure>`).join("\n");
  writeFileSync(
    join(outDir, "index.html"),
    `<!doctype html><meta charset="utf-8"><title>Reference pages</title><style>body{font:14px system-ui;margin:16px;background:#f4f4f5}main{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:12px}figure{margin:0;background:#fff;border:1px solid #ccc;padding:8px}img{width:100%;height:240px;object-fit:cover;object-position:top}small{color:#666;word-break:break-all}</style><h1>Reference pages (static files)</h1><main>${rows}</main>`,
  );
  console.log(`reference: ${done.length} screenshots in ${outDir}`);
  for (const s of skipped) console.log(`reference skipped: ${s}`);
}

export { THEMES };
