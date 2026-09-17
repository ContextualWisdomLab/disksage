export interface OnlineSetting {
  online_mode: boolean;
}

export interface CheckableControl {
  checked: boolean;
}

export type SaveOnlineSetting = (online: boolean) => Promise<OnlineSetting>;

/**
 * Persist one online-mode toggle without allowing the browser's eager checkbox state to pose as
 * durable application state.
 *
 * Native checkboxes flip `checked` before the change handler runs. Restore the last persisted
 * value immediately, then expose the backend-authored value only after persistence succeeds. On
 * failure, keep the visible control on the durable value and rethrow so the caller can surface an
 * actionable error without changing application state.
 */
export async function persistOnlineToggle(
  persistedOnline: boolean,
  checkbox: CheckableControl,
  saveOnlineSetting: SaveOnlineSetting,
): Promise<boolean> {
  checkbox.checked = persistedOnline;
  try {
    const settings = await saveOnlineSetting(!persistedOnline);
    checkbox.checked = settings.online_mode;
    return settings.online_mode;
  } catch (error) {
    checkbox.checked = persistedOnline;
    throw error;
  }
}
