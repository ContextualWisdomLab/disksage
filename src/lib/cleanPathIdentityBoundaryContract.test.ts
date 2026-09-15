import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const commandsSource = readFileSync('src-tauri/src/commands.rs', 'utf8');

function sliceBetween(source: string, startMarker: string, endMarker: string): string {
  const start = source.indexOf(startMarker);
  expect(start, `${startMarker} must exist`).toBeGreaterThanOrEqual(0);
  const end = source.indexOf(endMarker, start + startMarker.length);
  expect(end, `${endMarker} must follow ${startMarker}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe('review-bound identity for the buyer-visible clean path', () => {
  it('does not expose a raw pathname-only Tauri deletion request', () => {
    const command = sliceBetween(
      commandsSource,
      'pub fn clean_paths(',
      '#[tauri::command]\npub fn list_roots',
    );

    expect(command).not.toContain('paths: Vec<String>');
    expect(
      command,
      'the request crossing the UI/native boundary must carry the object identity captured at review time',
    ).toContain('expected_object_id');
  });

  it('does not downgrade reviewed identity to a fresh pathname lookup before Trash mutation', () => {
    const core = sliceBetween(
      commandsSource,
      'pub fn clean_paths_inner(',
      '/// 개발 아티팩트는 목록 시점의 bounded metadata manifest',
    );

    expect(core).not.toContain('paths: &[PathBuf]');
    expect(core).not.toContain('safety::trash_delete(');
    expect(
      core,
      'the clean core must consume the review-time object identity at the final safety boundary',
    ).toContain('safety::trash_delete_if_identity');
    expect(core).toContain('expected_object_id');
  });
});
