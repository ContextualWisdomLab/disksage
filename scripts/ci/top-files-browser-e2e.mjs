import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "vite";

const HOST = "127.0.0.1";
const FRONTEND_PORT = 4173;
const DEBUG_PORT = 9222;
const BASE_URL = `http://${HOST}:${FRONTEND_PORT}`;

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function findChrome() {
  const candidates = [
    process.env.CHROME_BIN,
    "/usr/bin/google-chrome-stable",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ].filter(Boolean);
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate;
  }
  for (const name of ["google-chrome-stable", "google-chrome", "chromium", "chromium-browser"]) {
    const found = spawnSync("sh", ["-lc", `command -v ${name}`], { encoding: "utf8" });
    if (found.status === 0 && found.stdout.trim()) return found.stdout.trim();
  }
  throw new Error("browser-e2e-chrome-unavailable");
}

async function waitForJson(url, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { redirect: "error" });
      if (response.ok) return await response.json();
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`browser-e2e-readiness-timeout: ${lastError?.message ?? "unknown"}`);
}

class CdpClient {
  constructor(url) {
    this.url = url;
    this.nextId = 1;
    this.pending = new Map();
    this.ws = null;
  }

  async connect() {
    this.ws = new WebSocket(this.url);
    await new Promise((resolve, reject) => {
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", () => reject(new Error("browser-e2e-cdp-connect-failed")), { once: true });
    });
    this.ws.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const waiter = this.pending.get(message.id);
      if (!waiter) return;
      this.pending.delete(message.id);
      if (message.error) waiter.reject(new Error(`${message.error.code}: ${message.error.message}`));
      else waiter.resolve(message.result ?? {});
    });
  }

  call(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  close() {
    this.ws?.close();
  }
}

const mockRuntime = String.raw`(() => {
  const callbacks = new Map();
  const eventListeners = new Map();
  let callbackId = 1;
  let eventId = 1;
  const scenario = new URL(location.href).searchParams.get("e2eScenario") || "normal";
  const longStem = "資料-데이터-datos-Daten-données-dữ-liệu-ファイル-文件-".repeat(8);
  const state = {
    scenario,
    scanStarts: 0,
    invokes: [],
    files: Array.from({ length: 120 }, (_, index) => ({
      name: "large-" + index + ".bin",
      path: "/e2e/" + longStem + index + ".bin",
      size: 10_000_000 - index,
      is_dir: false,
    })),
  };

  function runCallback(id, data) {
    const item = callbacks.get(id);
    if (!item) return;
    item.callback?.(data);
    if (item.once) callbacks.delete(id);
  }

  window.__TAURI_INTERNALS__ = {
    callbacks,
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback(callback, once = false) {
      const id = callbackId++;
      callbacks.set(id, { callback, once });
      return id;
    },
    unregisterCallback(id) { callbacks.delete(id); },
    runCallback,
    convertFileSrc(path) { return path; },
    async invoke(cmd, args = {}) {
      state.invokes.push({ cmd, args });
      if (cmd === "plugin:event|listen") {
        const id = eventId++;
        eventListeners.set(id, { event: args.event, callbackId: args.handler });
        return id;
      }
      if (cmd === "plugin:event|unlisten") {
        eventListeners.delete(args.eventId);
        return undefined;
      }
      if (cmd === "list_roots") {
        if (state.scenario === "permission") throw new Error("permission denied");
        return ["/e2e"];
      }
      if (cmd === "start_scan") {
        state.scanStarts += 1;
        return undefined;
      }
      if (cmd === "cancel_scan") return undefined;
      if (cmd === "get_node") {
        if (state.scenario === "post-scan-error") throw new Error("materialization denied");
        return {
          path: "/e2e",
          size: 10_000_000,
          entries: [
            { name: "folder", path: "/e2e/folder", size: 5_000_000, is_dir: true },
            { name: "file.bin", path: "/e2e/file.bin", size: 5_000_000, is_dir: false },
          ],
        };
      }
      if (cmd === "top_files") {
        if (state.scenario === "post-scan-error") throw new Error("top-files denied");
        return state.scenario === "empty" ? [] : state.files;
      }
      if (cmd === "list_cache_candidates" || cmd === "list_dev_artifacts" || cmd === "recent_operations" || cmd === "find_duplicate_files") return [];
      if (cmd === "plan_orphan_cleanup") return { schema_kind: "disksage.orphan-plan/v1", schema_version: 1, generated_at_ms: 1, plan_fingerprint: "0".repeat(64), candidate_count: 0, candidate_bytes: 0, scan_complete: true, candidates: [], notices: [], local_paths_included: false, mutation_performed: false, exact_approval_phrase: "" };
      return null;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(_event, id) { eventListeners.delete(id); },
  };
  window.__DISKSAGE_E2E__ = {
    emit(event, payload) {
      for (const [id, listener] of eventListeners) {
        if (listener.event === event) runCallback(listener.callbackId, { event, id, payload });
      }
    },
    state,
  };
})();`;

async function evaluate(cdp, expression, awaitPromise = true) {
  const result = await cdp.call("Runtime.evaluate", {
    expression,
    awaitPromise,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(`browser-e2e-evaluate-failed: ${result.exceptionDetails.text}`);
  }
  return result.result?.value;
}

async function waitFor(cdp, expression, label, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await evaluate(cdp, expression)) return;
    await new Promise((resolve) => setTimeout(resolve, 75));
  }
  throw new Error(`browser-e2e-timeout:${label}`);
}

async function navigate(cdp, scenario) {
  await cdp.call("Page.navigate", { url: `${BASE_URL}/?e2eScenario=${scenario}` });
  await waitFor(cdp, "document.readyState === 'complete'", `document-ready:${scenario}`);
  await waitFor(cdp, "!!window.__DISKSAGE_E2E__", `mock-ready:${scenario}`);
  if (scenario !== "permission") {
    await waitFor(cdp, "document.querySelector('select')?.value === '/e2e'", `roots:${scenario}`);
  }
}

async function clickScan(cdp) {
  const clicked = await evaluate(cdp, `(() => {
    const button = [...document.querySelectorAll('button')].find((value) => value.textContent?.trim() === '스캔');
    if (!button) return false;
    button.click();
    return true;
  })()`);
  assert(clicked, "browser-e2e-scan-button-missing");
  await waitFor(cdp, "[...document.querySelectorAll('button')].some((value) => value.textContent?.trim() === '취소')", "scan-loading-state");
  assert(!(await evaluate(cdp, "!!document.querySelector('#top-files-table')")), "top-files-surface-visible-during-loading");
}

async function finishScan(cdp) {
  await evaluate(cdp, `window.__DISKSAGE_E2E__.emit('scan://done', { files: 120, dirs: 1, skipped: 0, bytes: 10000000 })`);
}

async function pressKey(cdp, key, code, windowsVirtualKeyCode) {
  await cdp.call("Input.dispatchKeyEvent", { type: "keyDown", key, code, windowsVirtualKeyCode });
  await cdp.call("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode });
}

async function proveNormalInteraction(cdp) {
  await navigate(cdp, "normal");
  assert(!(await evaluate(cdp, "!!document.querySelector('#top-files-table')")), "top-files-surface-visible-before-scan");
  await clickScan(cdp);
  await finishScan(cdp);
  await waitFor(cdp, "!!document.querySelector('#top-files-table table tbody tr')", "normal-top-files");

  const structure = await evaluate(cdp, `(() => {
    const region = document.querySelector('#top-files-table');
    const table = region?.querySelector('table');
    return {
      role: region?.getAttribute('role'),
      tabIndex: region?.tabIndex,
      labelledBy: region?.getAttribute('aria-labelledby'),
      headers: [...(table?.querySelectorAll('th') ?? [])].map((node) => ({ text: node.textContent?.trim(), scope: node.getAttribute('scope') })),
      rows: table?.querySelectorAll('tbody tr').length ?? 0,
    };
  })()`);
  assert(structure.role === "region", "top-files-region-role-missing");
  assert(structure.tabIndex === 0, "top-files-region-not-sequentially-focusable");
  assert(structure.labelledBy === "top-files-heading", "top-files-region-label-missing");
  assert(structure.headers.length === 2 && structure.headers.every((header) => header.scope === "col"), "top-files-column-scope-missing");
  assert(structure.rows >= 100, "top-files-scroll-fixture-too-small");

  await evaluate(cdp, "document.body.focus()", false);
  let reached = false;
  for (let index = 0; index < 40; index += 1) {
    await pressKey(cdp, "Tab", "Tab", 9);
    if (await evaluate(cdp, "document.activeElement?.id === 'top-files-table'")) {
      reached = true;
      break;
    }
  }
  assert(reached, "top-files-region-not-reachable-by-tab");
  const focusStyle = await evaluate(cdp, `(() => {
    const style = getComputedStyle(document.querySelector('#top-files-table'));
    return { outlineStyle: style.outlineStyle, outlineWidth: style.outlineWidth };
  })()`);
  assert(focusStyle.outlineStyle !== "none" && focusStyle.outlineWidth !== "0px", "top-files-visible-focus-missing");

  const beforeScroll = await evaluate(cdp, "document.querySelector('#top-files-table').scrollTop");
  await pressKey(cdp, "PageDown", "PageDown", 34);
  await new Promise((resolve) => setTimeout(resolve, 100));
  const afterScroll = await evaluate(cdp, "document.querySelector('#top-files-table').scrollTop");
  assert(afterScroll > beforeScroll, "top-files-keyboard-scroll-failed");

  const shortcutClicked = await evaluate(cdp, `(() => {
    const link = [...document.querySelectorAll('a')].find((value) => value.textContent?.trim() === '파일 표 탐색 시작');
    if (!link) return false;
    link.click();
    return true;
  })()`);
  assert(shortcutClicked, "top-files-fragment-shortcut-missing");
  await waitFor(cdp, "location.hash === '#top-files-table'", "fragment-target");
  assert(await evaluate(cdp, "document.activeElement?.id === 'top-files-table'"), "top-files-fragment-does-not-focus-region");

  for (const width of [375, 768, 1024]) {
    await cdp.call("Emulation.setDeviceMetricsOverride", { width, height: 800, deviceScaleFactor: 1, mobile: false });
    await new Promise((resolve) => setTimeout(resolve, 50));
    const overflow = await evaluate(cdp, "document.documentElement.scrollWidth > document.documentElement.clientWidth");
    assert(!overflow, `top-files-page-horizontal-overflow:${width}`);
    const clipped = await evaluate(cdp, `(() => {
      const region = document.querySelector('#top-files-table');
      const heading = document.querySelector('#top-files-heading');
      if (!region || !heading) return true;
      const rr = region.getBoundingClientRect();
      const hr = heading.getBoundingClientRect();
      return rr.right > innerWidth + 1 || hr.right > innerWidth + 1 || rr.left < -1 || hr.left < -1;
    })()`);
    assert(!clipped, `top-files-critical-content-clipped:${width}`);
  }
  await cdp.call("Emulation.clearDeviceMetricsOverride");
}

async function proveEmptyAndErrorStates(cdp) {
  await navigate(cdp, "empty");
  await clickScan(cdp);
  await finishScan(cdp);
  await waitFor(cdp, "!!document.querySelector('[role=status]')", "empty-status");
  const emptyText = await evaluate(cdp, "document.querySelector('[role=status]')?.textContent?.trim() || ''");
  assert(emptyText.includes("다시 스캔"), "top-files-empty-recovery-instruction-missing");
  assert(!(await evaluate(cdp, "!!document.querySelector('#top-files-table')")), "top-files-empty-state-exposes-table");

  await navigate(cdp, "post-scan-error");
  await clickScan(cdp);
  await finishScan(cdp);
  await waitFor(cdp, "!!document.querySelector('[role=alert]')", "post-scan-error-alert");
  assert(!(await evaluate(cdp, "!!document.querySelector('#top-files-table')")), "top-files-error-state-exposes-table");
}

async function provePermissionState(cdp) {
  await navigate(cdp, "permission");
  await waitFor(cdp, "!!document.querySelector('[role=alert]')", "permission-alert");
  const scanEnabledWithoutRoot = await evaluate(cdp, `(() => {
    const button = [...document.querySelectorAll('button')].find((value) => value.textContent?.trim() === '스캔');
    return !!button && !button.disabled;
  })()`);
  assert(!scanEnabledWithoutRoot, "permission-state-leaves-inert-scan-control-enabled");
}

async function main() {
  const server = await createServer({
    server: { host: HOST, port: FRONTEND_PORT, strictPort: true },
    logLevel: "error",
  });
  let chrome;
  let cdp;
  let profile;
  try {
    await server.listen();
    const chromeBinary = findChrome();
    profile = mkdtempSync(join(tmpdir(), "disksage-browser-e2e-"));
    chrome = spawn(chromeBinary, [
      "--headless=new",
      ...(typeof process.getuid === "function" && process.getuid() === 0
        ? ["--no-sandbox", "--disable-setuid-sandbox"]
        : []),
      `--remote-debugging-port=${DEBUG_PORT}`,
      `--user-data-dir=${profile}`,
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-default-apps",
      "--disable-dev-shm-usage",
      "--disable-sync",
      "--metrics-recording-only",
      "--no-first-run",
      "about:blank",
    ], { stdio: ["ignore", "pipe", "pipe"] });

    const targets = await waitForJson(`http://${HOST}:${DEBUG_PORT}/json/list`);
    const page = targets.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
    assert(page, "browser-e2e-page-target-unavailable");
    cdp = new CdpClient(page.webSocketDebuggerUrl);
    await cdp.connect();
    await cdp.call("Page.enable");
    await cdp.call("Runtime.enable");
    await cdp.call("Emulation.setFocusEmulationEnabled", { enabled: true });
    await cdp.call("Page.addScriptToEvaluateOnNewDocument", { source: mockRuntime });

    await proveNormalInteraction(cdp);
    await proveEmptyAndErrorStates(cdp);
    await provePermissionState(cdp);
    console.log("DISKSAGE_BROWSER_E2E_RESULT status=passed browser=chrome scenarios=normal,loading,empty,error,permission");
  } finally {
    cdp?.close();
    chrome?.kill("SIGTERM");
    await server.close();
    if (profile) rmSync(profile, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack : error);
  process.exitCode = 1;
});