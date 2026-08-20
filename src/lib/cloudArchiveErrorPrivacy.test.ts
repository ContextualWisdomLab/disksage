import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  boundedCloudArchiveErrorMessage,
  type CloudArchiveErrorOperation,
} from "./cloudArchiveErrorFeedback";

const operations = [
  "initialize",
  "preview",
  "review",
  "copy",
  "provider-api-copy",
  "adopt",
  "attest",
  "reconcile",
  "icloud-health",
  "provider-sync",
  "provider-recovery",
  "evict",
  "capacity",
  "connect",
  "disconnect",
] as const satisfies readonly CloudArchiveErrorOperation[];

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("CloudArchive bounded error feedback", () => {
  it("drops arbitrary backend details for every user-visible failure phase", () => {
    const sensitiveDetail =
      "OAuth refresh failed for /Users/alice/private/report.pdf token=sk-sensitive";

    for (const operation of operations) {
      const message = boundedCloudArchiveErrorMessage(
        operation,
        new Error(sensitiveDetail),
      );

      expect(message.length).toBeGreaterThan(0);
      expect(message).not.toContain("/Users/alice");
      expect(message).not.toContain("report.pdf");
      expect(message).not.toContain("sk-sensitive");
      expect(message).not.toContain("OAuth refresh failed");
    }
  });

  it("keeps operation-specific guidance instead of collapsing every failure", () => {
    const messages = new Set(
      operations.map((operation) => boundedCloudArchiveErrorMessage(operation, "backend detail")),
    );

    expect(messages.size).toBe(operations.length);
  });

  it("routes every CloudArchive catch boundary through bounded feedback", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/CloudArchive.svelte"), "utf8");

    expect(source).not.toContain("String(e)");
    for (const operation of operations) {
      expect(source).toContain(`boundedCloudArchiveErrorMessage(\"${operation}\"`);
    }
  });
});
