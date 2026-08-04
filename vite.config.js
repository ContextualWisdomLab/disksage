import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { lookup } from "node:dns/promises";
import { isIP } from "node:net";

const WILDCARD_BIND_ADDRESSES = new Set(["0.0.0.0", "::", "::ffff:0:0"]);

/** @param {string} address */
function normalizeIpAddress(address) {
  const candidate = address.trim();
  const authority = candidate.includes(":") && !candidate.startsWith("[") ? `[${candidate}]` : candidate;
  let parsed;
  try {
    parsed = new URL(`http://${authority}`);
  } catch {
    throw new Error("TAURI_DEV_HOST resolved to an invalid IP address");
  }
  const normalized = parsed.hostname.toLowerCase().replace(/^\[|\]$/gu, "");
  if (!isIP(normalized)) {
    throw new Error("TAURI_DEV_HOST resolved to an invalid IP address");
  }
  return normalized;
}

/** @param {string} hostname */
async function lookupAll(hostname) {
  return lookup(hostname, { all: true, verbatim: true });
}

/**
 * @param {string | undefined} requestedHost
 * @param {(hostname: string) => Promise<Array<{ address: string, family: number }>>} [resolveAll]
 */
export async function resolveDevHost(requestedHost, resolveAll = lookupAll) {
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

  const normalizedHost = parsed.hostname.toLowerCase().replace(/^\[|\]$/gu, "");
  if (normalizedHost === "*") {
    throw new Error("TAURI_DEV_HOST must not bind all network interfaces");
  }

  let addresses;
  if (isIP(normalizedHost)) {
    addresses = [{ address: normalizedHost, family: isIP(normalizedHost) }];
  } else {
    try {
      addresses = await resolveAll(normalizedHost);
    } catch {
      throw new Error("TAURI_DEV_HOST must resolve to a usable IP address");
    }
  }
  if (!Array.isArray(addresses) || addresses.length === 0) {
    throw new Error("TAURI_DEV_HOST must resolve to a usable IP address");
  }

  const normalizedAddresses = addresses.map(({ address }) => normalizeIpAddress(address));
  if (normalizedAddresses.some((address) => WILDCARD_BIND_ADDRESSES.has(address))) {
    throw new Error("TAURI_DEV_HOST must not bind all network interfaces");
  }

  // Bind to the validated numeric result, not the original DNS name, so the listener cannot
  // resolve a different address between validation and startup.
  return normalizedAddresses[0];
}

// https://vite.dev/config/
export default defineConfig(async () => {
  const host = await resolveDevHost(process.env.TAURI_DEV_HOST);
  return {
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
  };
});
