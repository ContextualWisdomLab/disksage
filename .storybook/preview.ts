import type { Preview } from "storybook";
import "../src/lib/ui/design-tokens.css";

const preview: Preview = {
  parameters: {
    a11y: {
      test: "error",
    },
    controls: {
      expanded: true,
    },
    viewport: {
      viewports: {
        desktop: { name: "Desktop", styles: { width: "1280px", height: "800px" } },
        mobile: { name: "Mobile", styles: { width: "375px", height: "812px" } },
      },
    },
  },
  tags: ["autodocs"],
};

export default preview;
