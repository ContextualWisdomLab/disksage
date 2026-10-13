import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  boundedCloudArchiveErrorMessage,
  isCloudCopyCancelled,
  type CloudArchiveErrorOperation,
} from "./cloudArchiveErrorFeedback";

const operations = [
  "initialize",
  "preview",
  "review",
  "copy",
  "cancel",
  "provider-api-copy",
  "adopt",
  "attest",
  "reconcile",
  "icloud-health",
  "finder-copy-cancel",
  "provider-sync",
  "provider-recovery",
  "evict",
  "capacity",
  "connect",
  "disconnect",
] as const satisfies readonly CloudArchiveErrorOperation[];

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("CloudArchive bounded error feedback", () => {
  it("recognizes only the deliberate cloud-copy cancellation outcome", () => {
    expect(isCloudCopyCancelled("cloud-copy-cancelled")).toBe(true);
    expect(isCloudCopyCancelled("cloud-copy-cancelled;failure-record-write-failed")).toBe(true);
    expect(isCloudCopyCancelled(new Error("cloud-copy-cancelled"))).toBe(true);
    expect(isCloudCopyCancelled({ message: "cloud-copy-cancelled" })).toBe(true);
    expect(isCloudCopyCancelled("failure-record-write-failed")).toBe(false);
    expect(isCloudCopyCancelled(new Error("cloud-copy-failed"))).toBe(false);
  });

  it("treats a late cancel after the native operation finished as a benign no-op", () => {
    expect(boundedCloudArchiveErrorMessage("cancel", "cloud-copy-not-active")).toBe("");
    expect(boundedCloudArchiveErrorMessage("cancel", new Error("cloud-copy-not-active"))).toBe("");
    expect(boundedCloudArchiveErrorMessage("cancel", { message: "cloud-copy-not-active" })).toBe("");
    expect(boundedCloudArchiveErrorMessage("cancel", "cloud-copy-operation-mismatch")).not.toBe("");
  });

  it("does not invoke untrusted object traps while classifying caught values", () => {
    let accessorInvoked = false;
    const accessorError = Object.defineProperty({}, "message", {
      configurable: true,
      get() {
        accessorInvoked = true;
        throw new Error("secret getter detail");
      },
    });

    expect(boundedCloudArchiveErrorMessage("cancel", accessorError)).not.toBe("");
    expect(isCloudCopyCancelled(accessorError)).toBe(false);
    expect(accessorInvoked).toBe(false);

    const descriptorProxy = new Proxy(
      {},
      {
        getOwnPropertyDescriptor() {
          throw new Error("secret descriptor detail");
        },
      },
    );
    expect(boundedCloudArchiveErrorMessage("copy", descriptorProxy)).toBe(
      "클라우드 복사를 실행하지 못했습니다. 연결 상태와 대상 위치를 확인한 뒤 다시 시도하십시오.",
    );
    expect(isCloudCopyCancelled(descriptorProxy)).toBe(false);

    const prototypeProxy = new Proxy(
      {},
      {
        getPrototypeOf() {
          throw new Error("secret prototype detail");
        },
      },
    );
    expect(boundedCloudArchiveErrorMessage("copy", prototypeProxy)).toBe(
      "클라우드 복사를 실행하지 못했습니다. 연결 상태와 대상 위치를 확인한 뒤 다시 시도하십시오.",
    );
    expect(isCloudCopyCancelled(prototypeProxy)).toBe(false);
  });

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

  it("keeps every visible failure message actionable and implementation-neutral", () => {
    const forbidden = ["공급자", "OAuth", "attestation", "File Provider", "eviction permit"];
    const nextAction = /(확인|다시|새로고침|기다|연결|열어|권한|시도)/;

    for (const operation of operations) {
      const message = boundedCloudArchiveErrorMessage(operation, "backend detail");
      for (const term of forbidden) expect(message).not.toContain(term);
      expect(message).toMatch(nextAction);
    }
  });

  it("routes every CloudArchive catch boundary through bounded feedback", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/CloudArchive.svelte"), "utf8");

    expect(source).not.toContain("String(e)");
    for (const operation of operations) {
      expect(source).toContain(`boundedCloudArchiveErrorMessage(\"${operation}\"`);
    }
  });
});
