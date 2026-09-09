# AGENTS.md

Before changing product behavior, read the canonical [product requirements](docs/PRD.md), then use
the [architecture index](ARCHITECTURE.md) for technical ownership and the
[product and technical gap baseline](docs/product-technical-gap-baseline.md) for current open work.
Do not turn implementation names or raw diagnostic codes into customer-facing explanations; every
customer message must state the condition and the next safe action.

## Code-owner review gates — disabled (on hold)

As of 2026-08-04, code-owner review requirements (`require_code_owner_reviews` in branch
protection, `require_code_owner_review` in rulesets) are disabled across the ContextualWisdomLab
org: there is a single maintainer (solo developer), so a code-owner approval gate can never be
satisfied. This is ON HOLD until the org has multiple maintainers — do NOT re-enable these
settings or add CODEOWNERS-based merge gates before then.
