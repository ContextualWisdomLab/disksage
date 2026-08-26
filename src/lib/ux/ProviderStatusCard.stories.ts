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
    await expect(canvas.queryByRole("button", { name: "복사 취소 요청" })).not.toBeInTheDocument();
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
    details: "클라우드 확인이 진행률 없이 멈춰 새 복사와 원본 정리를 차단했습니다.",
    observedAt: "2026-08-21 17:07 KST",
    blockedFor: "23분",
    canCancel: true,
    onCancel: fn(),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    expect(canvasElement.ownerDocument.defaultView?.innerWidth).toBe(375);
    await expect(canvas.getByRole("status")).toHaveTextContent("파일 준비 지연");
    await userEvent.click(canvas.getByRole("button", { name: "복사 취소 요청" }));
    await expect(args.onCancel).toHaveBeenCalledOnce();
  },
};

export const CheckingWithoutAction: Story = {
  args: {
    statusId: "checking-provider-status",
    headingLevel: "h1",
    provider: "Google Drive",
    state: "checking",
    details: "클라우드 상태를 읽기 전용으로 확인하고 있습니다.",
    canCancel: true,
    onCancel: fn(),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("button", { name: "복사 취소 요청" })).toBeDisabled();
    await userEvent.click(canvas.getByRole("button", { name: "복사 취소 요청" }));
    await expect(args.onCancel).not.toHaveBeenCalled();
  },
};

export const ProbeInFlight: Story = {
  args: {
    statusId: "in-flight-provider-status",
    headingLevel: "h1",
    provider: "OneDrive",
    state: "provider-sync-incomplete",
    details: "새 확인을 기다리는 동안 이전 차단 상태를 표시합니다.",
    canCancel: true,
    cancelDisabled: true,
    onCancel: fn(),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole("button", { name: "복사 취소 요청" })).toBeDisabled();
    await userEvent.click(canvas.getByRole("button", { name: "복사 취소 요청" }));
    await expect(args.onCancel).not.toHaveBeenCalled();
  },
};

export const IncompleteEvidence: Story = {
  args: {
    statusId: "incomplete-provider-status",
    headingLevel: "h1",
    provider: "OneDrive",
    state: "provider-sync-incomplete",
    details: "클라우드 상태를 확인할 수 없어 기존 목적지를 채택하지 않습니다.",
    observedAt: "2026-08-21 17:30 KST",
  },
};
