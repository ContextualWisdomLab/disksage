import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const commands = readFileSync('src-tauri/src/commands.rs', 'utf8');
const navigation = readFileSync('src-tauri/src/node_navigation.rs', 'utf8');
const lib = readFileSync('src-tauri/src/lib.rs', 'utf8');

describe('scan-tree navigation single-owner contract', () => {
  it('keeps the identity-aware node_navigation boundary as the only get_node implementation', () => {
    expect(navigation).toMatch(/#\[tauri::command\(rename = "get_node"\)\][\s\S]*pub\(crate\) fn get_node_secure\s*\(/);
    expect(lib).toContain('node_navigation::get_node_secure,');
    expect(lib).not.toContain('commands::get_node,');

    expect(commands).not.toMatch(/\bpub fn node_view\s*\(/);
    expect(commands).not.toMatch(/#\[tauri::command\]\s*\npub fn get_node\s*\(/);
  });
});
