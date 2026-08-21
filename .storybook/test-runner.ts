import { getStoryContext, type TestRunnerConfig } from "@storybook/test-runner";

const config: TestRunnerConfig = {
  async preVisit(page, story) {
    const context = await getStoryContext(page, story);
    if (context.parameters?.viewport?.defaultViewport === "mobile") {
      await page.setViewportSize({ width: 375, height: 812 });
    }
  },
};

export default config;
