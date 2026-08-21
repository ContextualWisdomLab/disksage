import { getStoryContext, type TestRunnerConfig } from "@storybook/test-runner";

const MOBILE_VIEWPORT = { width: 375, height: 812 };
const DESKTOP_VIEWPORT = { width: 1280, height: 800 };
type StoryGlobals = { viewport?: { value?: string } };

const config: TestRunnerConfig = {
  async preVisit(page, story) {
    const context = await getStoryContext(page, story);
    const globals = (context as unknown as { globals?: StoryGlobals }).globals;
    const viewport =
      globals?.viewport?.value === "mobile"
        ? MOBILE_VIEWPORT
        : DESKTOP_VIEWPORT;
    await page.setViewportSize(viewport);
  },
};

export default config;
