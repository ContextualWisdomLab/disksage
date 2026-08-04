# Temporary iCloud batch repair validation trigger

This exact-head marker exists only to trigger the repository's ordinary pull-request test workflow while the read-only repair artifact is produced. It must be removed before the pull request is eligible to merge.

The current validation pass is intentionally triggered by a maintainer-authored commit so the temporary fail-closed jobs execute once and then publish only their digest-verified Rust source artifact.