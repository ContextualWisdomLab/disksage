import { describe, expect, it } from "vitest";
import { ssr } from "./+layout";

describe("static desktop layout contract", () => {
  it("keeps server-side rendering disabled for the Tauri SPA", () => {
    expect(ssr).toBe(false);
  });
});
