import { existsSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const modulePath = 'src-tauri/src/uv_cache_reclaim.rs';
const cliPath = 'src-tauri/src/bin/disksage-uv-cache-reclaim.rs';
const fixturePath = 'src-tauri/tests/uv_cache_reclaim_cli_exit.rs';
const lib = readFileSync('src-tauri/src/lib.rs', 'utf8');
const cargo = readFileSync('src-tauri/Cargo.toml', 'utf8');

describe('native uv cache reclaim owner boundary', () => {
  it('routes uv mutation through a dedicated native owner', () => {
    expect(existsSync(modulePath)).toBe(true);
    expect(existsSync(cliPath)).toBe(true);
    expect(existsSync(fixturePath)).toBe(true);
    expect(lib).toMatch(/pub\s+mod\s+uv_cache_reclaim\s*;/);
    expect(cargo).toContain('name = "disksage-uv-cache-reclaim"');

    if (!existsSync(modulePath)) return;
    const source = readFileSync(modulePath, 'utf8');
    expect(source).toContain('"cache"');
    expect(source).toContain('"prune"');
    expect(source).toContain('UV_LOCK_TIMEOUT');
    expect(source).not.toContain('"--force"');
    expect(source).not.toContain('remove_dir_all');
    expect(source).not.toContain('trash_delete');
    expect(source).toContain('persistent-service-cache-dependency');
    expect(source).toContain('persistent-service-evidence-incomplete');
  });

  it('keeps destructive acceptance on real filesystem/process evidence', () => {
    if (!existsSync(fixturePath)) return;
    const fixture = readFileSync(fixturePath, 'utf8');
    expect(fixture).toContain('open_cached_payload_blocks_native_prune');
    expect(fixture).toContain('persistent_service_reference_blocks_native_prune');
    expect(fixture).toContain('native_prune_invokes_uv_not_private_bucket_deletion');
  });
});
