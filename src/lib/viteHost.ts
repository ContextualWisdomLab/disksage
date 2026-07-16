const LOOPBACK_HOSTS = new Set(["localhost", "127.0.0.1", "::1", "[::1]"]);

/**
 * Keep the unauthenticated Vite and HMR development listeners on loopback.
 *
 * Tauri may provide `TAURI_DEV_HOST`, but wildcard, LAN, and public host values would expose
 * source modules and HMR to the network. Remote-device development must use an authenticated,
 * access-controlled tunnel instead of changing this listener.
 */
export function safeTauriDevHost(configured: string | undefined): string | false {
  const host = configured?.trim().toLowerCase();
  return host && LOOPBACK_HOSTS.has(host) ? host : false;
}
