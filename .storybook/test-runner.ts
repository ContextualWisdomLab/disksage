import { getStoryContext, type TestRunnerConfig } from "@storybook/test-runner";

const MOBILE_VIEWPORT = { width: 375, height: 812 };
const DESKTOP_VIEWPORT = { width: 1280, height: 800 };
type StoryGlobals = { viewport?: { value?: string } };
type StoryContextWithGlobals = { storyGlobals?: StoryGlobals };

const config: TestRunnerConfig = {
  async preVisit(page, story) {
    const context = await getStoryContext(page, story);
    const storyGlobals = (context as unknown as StoryContextWithGlobals).storyGlobals;
    const viewport =
      storyGlobals?.viewport?.value === "mobile"
        ? MOBILE_VIEWPORT
        : DESKTOP_VIEWPORT;
    await page.setViewportSize(viewport);
  },
};

export default config;
