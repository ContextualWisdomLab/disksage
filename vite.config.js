import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

/** @param {string | undefined} requestedHost */
export function resolveDevHost(requestedHost) {
  const host = requestedHost?.trim();
  if (!host) return undefined;
  if (/[\\/@?#]/u.test(host)) {
    throw new Error("TAURI_DEV_HOST must contain only a host name or IP address");
  }

  const authority = host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  let parsed;
  try {
    parsed = new URL(`http://${authority}`);
  } catch {
    throw new Error("TAURI_DEV_HOST must contain only a host name or IP address");
  }
  if (
    parsed.username ||
    parsed.password ||
    parsed.port ||
    parsed.pathname !== "/" ||
    parsed.search ||
    parsed.hash
  ) {
    throw new Error("TAURI_DEV_HOST must contain only a host name or IP address");
  }

  const normalized = parsed.hostname.toLowerCase();
  if (normalized === "0.0.0.0" || normalized === "[::]" || normalized === "*") {
    throw new Error("TAURI_DEV_HOST must not bind all network interfaces");
  }
  return host;
}

const host = resolveDevHost(process.env.TAURI_DEV_HOST);

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [sveltekit()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
