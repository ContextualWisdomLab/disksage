import type { Meta, StoryObj } from "@storybook/sveltekit";
import { expect, fn, userEvent, within } from "storybook/test";
import ProviderStatusCard from "./ProviderStatusCard.svelte";

const meta = {
  title: "DiskSage/ProviderStatusCard",
  component: ProviderStatusCard,
  tags: ["autodocs"],
  argTypes: {
    state: {
      control: "select",
      options: ["clear", "checking", "provider-sync-incomplete", "materialization-stalled"],
    },
    canCancel: { control: "boolean" },
  },
} satisfies Meta<typeof ProviderStatusCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Clear: Story = {
  args: {
    statusId: "clear-provider-status",
    headingLevel: "h1",
    provider: "iCloud",
    state: "clear",
    details: "새 복사는 허용할 수 있지만 파일 확인은 별도로 필요합니다.",
    observedAt: "2026-08-21 17:30 KST",
    canCancel: true,
    onCancel: fn(),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.queryByRole("button", { name: "Finder 복사 취소" })).not.toBeInTheDocument();
    await expect(args.onCancel).not.toHaveBeenCalled();
  },
};

export const MaterializationStalled: Story = {
  globals: { viewport: { value: "mobile", isRotated: false } },
  args: {
    statusId: "stalled-provider-status",
    headingLevel: "h1",
    provider: "iCloud",
    state: "materialization-stalled",
    details: "Finder 복사를 취소하고 잠시 후 상태를 다시 확인하세요. 완료 전에는 원본을 정리하지 않습니다.",
    observedAt: "2026-08-21 17:07 KST",
    blockedFor: "23분",
    canCancel: true,
    onCancel: fn(),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    expect(canvasElement.ownerDocument.defaultView?.innerWidth).toBe(375);
    expect(canvasElement.ownerDocument.documentElement.scrollWidth).toBeLessThanOrEqual(375);
    expect(canvasElement.getBoundingClientRect().right).toBeLessThanOrEqual(375);
    await expect(canvas.getByRole("status")).toHaveTextContent("파일 준비 지연");
    await userEvent.click(canvas.getByRole("button", { name: "Finder 복사 취소" }));
    await expect(args.onCancel).toHaveBeenCalledOnce();
  },
};

export const CheckingWithoutAction: Story = {
  args: {
    statusId: "checking-provider-status",
    headingLevel: "h1",
    provider: "Google Drive",
    state: "checking",
    details: "확인이 끝날 때까지 기다리세요. 이 화면에서는 파일을 변경하지 않습니다.",
    canCancel: true,
    onCancel: fn(),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("button", { name: "Finder 복사 취소" })).toBeDisabled();
    await userEvent.click(canvas.getByRole("button", { name: "Finder 복사 취소" }));
    await expect(args.onCancel).not.toHaveBeenCalled();
  },
};

export const ProbeInFlight: Story = {
  args: {
    statusId: "in-flight-provider-status",
    headingLevel: "h1",
    provider: "OneDrive",
    state: "provider-sync-incomplete",
    details: "새 확인이 끝날 때까지 기다리세요. 이전 상태가 표시되는 동안 새 복사는 보류됩니다.",
    canCancel: true,
    cancelDisabled: true,
    onCancel: fn(),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("button", { name: "Finder 복사 취소" })).toBeDisabled();
    await userEvent.click(canvas.getByRole("button", { name: "Finder 복사 취소" }));
    await expect(args.onCancel).not.toHaveBeenCalled();
  },
};

export const IncompleteEvidence: Story = {
  args: {
    statusId: "incomplete-provider-status",
    headingLevel: "h1",
    provider: "OneDrive",
    state: "provider-sync-incomplete",
    details: "클라우드 앱과 연결을 확인한 뒤 상태를 다시 확인하세요. 확인 전에는 원본을 정리하지 않습니다.",
    observedAt: "2026-08-21 17:30 KST",
  },
};
