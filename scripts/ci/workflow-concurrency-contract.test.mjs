import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workflow = (name) => readFileSync(new URL(`../../.github/workflows/${name}`, import.meta.url), "utf8");

test("PR validation cancels only superseded first-attempt heads from the same repository PR", () => {
  const source = workflow("test.yml");
  assert.match(source, /group: \$\{\{ github\.workflow \}\}-\$\{\{ github\.repository \}\}-\$\{\{ github\.event\.pull_request\.number \|\| github\.run_id \}\}/);
  assert.match(source, /cancel-in-progress: \$\{\{ github\.event_name == 'pull_request' && github\.run_attempt == 1 \}\}/);
});

test("release validation cancels only superseded runs from the same pull request", () => {
  const source = workflow("release.yml");
  assert.match(source, /group: \$\{\{ github\.workflow \}\}-\$\{\{ github\.repository \}\}-\$\{\{ github\.event\.pull_request\.number \|\| github\.run_id \}\}/);
  assert.match(source, /cancel-in-progress: \$\{\{ github\.event_name == 'pull_request' \}\}/);
});
