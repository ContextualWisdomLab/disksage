import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workflow = (name) => readFileSync(new URL(`../../.github/workflows/${name}`, import.meta.url), "utf8");

test("release validation cancels only superseded first-attempt PR heads", () => {
  const source = workflow("release.yml");
  assert.match(source, /group: \$\{\{ github\.workflow \}\}-\$\{\{ github\.repository \}\}-\$\{\{ github\.event\.pull_request\.number \|\| github\.ref \}\}/);
  assert.match(source, /cancel-in-progress: \$\{\{ github\.event_name == 'pull_request' && github\.run_attempt == 1 \}\}/);
});
