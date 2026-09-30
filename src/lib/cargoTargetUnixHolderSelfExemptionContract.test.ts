import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/unix_holder_authority.rs', 'utf8');

describe('Unix exact-object holder self exemption', () => {
  it('does not exempt every lsof record owned by the DiskSage process', () => {
    expect(
      source,
      'the DiskSage PID may hold reviewed descendants itself; a blanket PID exemption would authorize cleanup while those holders remain live',
    ).not.toMatch(/if\s+record\.pid\s*==\s*self_pid\s*\{\s*return\s+Ok\(\(\)\);\s*\}/s);
  });

  it('exempts only the exact retained root descriptor and root filesystem identity', () => {
    expect(source).toContain('retained_root_fd');
    expect(source).toContain('retained_root_identity');
    expect(source).toMatch(/record\.pid\s*==\s*self_pid[\s\S]*descriptor[\s\S]*retained_root_fd/);
    expect(source).toMatch(/record\.pid\s*==\s*self_pid[\s\S]*(device|identity)[\s\S]*retained_root_identity/);
  });

  it('keeps executable Rust regressions for the narrow exemption boundary', () => {
    expect(source).toContain('exact_retained_root_fd_is_exempted');
    expect(source).toContain('self_descendant_holder_blocks_authorization');
  });
});
