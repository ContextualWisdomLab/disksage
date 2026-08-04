# Temporary iCloud batch repair validation trigger

This exact-head marker exists only to trigger the repository's ordinary pull-request test workflow while the read-only repair artifact is produced. It must be removed before the pull request is eligible to merge.

The validation pass is intentionally triggered by a maintainer-authored commit so the temporary fail-closed jobs execute once, prove the regression tests red before the source repair and green afterward, and upload only their digest-verified Rust source artifact.

The final pull-request diff must contain the two reviewed Rust source repairs and no temporary workflow, patch script, or validation marker.