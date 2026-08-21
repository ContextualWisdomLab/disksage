import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@storybook/test-runner", () => ({
  getStoryContext: vi.fn(),
}));

import { getStoryContext } from "@storybook/test-runner";
import config from "../../.storybook/test-runner";

const mockedGetStoryContext = vi.mocked(getStoryContext);

function story(id: string) {
  return { id } as never;
}

describe("Storybook test-runner viewport isolation", () => {
  beforeEach(() => {
    mockedGetStoryContext.mockReset();
  });

  it("restores the desktop viewport after a mobile story", async () => {
    const setViewportSize = vi.fn();
    const page = { setViewportSize } as never;

    mockedGetStoryContext
      .mockResolvedValueOnce({
        parameters: { viewport: { defaultViewport: "mobile" } },
      } as never)
      .mockResolvedValueOnce({ parameters: {} } as never);

    await config.preVisit?.(page, story("mobile"));
    await config.preVisit?.(page, story("desktop"));

    expect(setViewportSize).toHaveBeenNthCalledWith(1, {
      width: 375,
      height: 812,
    });
    expect(setViewportSize).toHaveBeenNthCalledWith(2, {
      width: 1280,
      height: 800,
    });
  });
});
