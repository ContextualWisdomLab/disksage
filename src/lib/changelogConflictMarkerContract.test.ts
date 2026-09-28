import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const changelog = readFileSync('CHANGELOG.md', 'utf8');

describe('protected changelog merge integrity', () => {
  it('contains no unresolved merge markers', () => {
    expect(changelog).not.toContain('<<<<<<<');
    expect(changelog).not.toContain('=======');
    expect(changelog).not.toContain('>>>>>>>');
  });

  it('retains both protected-main cloud and Orca reclaim authority entries', () => {
    expect(changelog).toContain('Default-exclude app-managed libraries from cloud offload planning');
    expect(changelog).toContain('Encode Orca reclaim protection criteria for `git-worktree-audit` and `dev-artifacts`');
  });
});
