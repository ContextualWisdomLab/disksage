import { describe, expect, it, vi } from "vitest";
import { persistOnlineToggle } from "./settingsPersistenceFlow";

describe("settings persistence toggle behavior", () => {
  it("restores the persisted checkbox state before saving and commits the backend result", async () => {
    const checkbox = { checked: true };
    const save = vi.fn(async (requestedOnline: boolean) => {
      expect(checkbox.checked).toBe(false);
      expect(requestedOnline).toBe(true);
      return { online_mode: true };
    });

    const online = await persistOnlineToggle(false, checkbox, save);

    expect(save).toHaveBeenCalledOnce();
    expect(online).toBe(true);
    expect(checkbox.checked).toBe(true);
  });

  it("restores the persisted checkbox state when persistence fails", async () => {
    const checkbox = { checked: false };
    const failure = new Error("disk-full");
    const save = vi.fn(async (requestedOnline: boolean) => {
      expect(checkbox.checked).toBe(true);
      expect(requestedOnline).toBe(false);
      throw failure;
    });

    await expect(persistOnlineToggle(true, checkbox, save)).rejects.toBe(failure);

    expect(save).toHaveBeenCalledOnce();
    expect(checkbox.checked).toBe(true);
  });
});
