# Homebrew stale / orphan software audit

`disksage-homebrew-audit` is a read-only Rust command that inventories Homebrew
formula leaves (`brew leaves -r`) and installed casks, then classifies each
package as `stale`, `orphan`, `in-use`, or `unknown` with stable reason codes.

```bash
cargo run --manifest-path src-tauri/Cargo.toml \
  --bin disksage-homebrew-audit -- \
    --repo-root "$HOME/orca/workspaces" \
    --repo-root "$HOME/Documents" \
    --stale-after-days 90 \
    --name jmeter \
    --output "/absolute/new/private-report.json"
```

## Evidence gathered per package

- installed size (bounded `du` of Cellar/Caskroom/opt prefix)
- install time from `brew info --json=v2`
- last-use evidence:
  - formulae: executable `atime` under `opt/<name>/{bin,sbin}`
  - casks: `.app` `kMDItemLastUsedDate` via Spotlight (`mdls`)
- reverse dependencies via `brew uses --installed`
- orphaned dependency candidates via `brew autoremove --dry-run`
- running processes under the package prefix (`lsof +D`, bounded)
- repository toolchain references under user-configured `--repo-root` values
  (`.tool-versions`, `mise.toml`, `Brewfile`, `Dockerfile`, CI yaml,
  `pyproject.toml`, `package.json`, and related manifests)

## Classification rules

Classification is fail-closed and **never** becomes `stale` from atime alone:

| Class | When |
| --- | --- |
| `in-use` | running process, repository reference, reverse dependency, or last-use within threshold |
| `orphan` | `brew autoremove --dry-run` candidate that is not a requested leaf |
| `stale` | leaf/cask with reliable last-use older than `--stale-after-days`, no reverse deps, no running processes, no repo references |
| `unknown` | missing/unreliable last-use evidence, incomplete brew probes, or other evidence gaps |

Stable reason codes include `running-process-from-prefix`,
`referenced-by-repository-toolchain`, `has-reverse-dependencies`,
`autoremove-orphan-candidate`, `last-use-evidence-missing`, `atime-unreliable`,
`last-use-within-threshold`, `last-use-exceeds-threshold`, and
`install-time-only-no-use-evidence`.

The command never uninstalls packages. A later uninstall path must re-check
identity/evidence and require separate human approval.
