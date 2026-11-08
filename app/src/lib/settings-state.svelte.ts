import type { Backend } from './backend';
import { errText } from './postproc';
import type { Settings, SettingsPatch } from './types';

export type SettingsState = ReturnType<typeof createSettingsState>;

/**
 * The Settings page's data. Each change saves on its own; a refused save keeps the last saved
 * value and shows the message under that field until the field saves.
 */
export function createSettingsState(backend: Backend, ui: { onChanged?: (p: SettingsPatch) => void }) {
  let settings = $state<Settings | null>(null);
  let error = $state<Partial<Record<keyof Settings, string>>>({});
  // Bumped after a refused save so inputs re-render with the saved value.
  let rev = $state(0);

  async function load() {
    settings = await backend.settings();
  }

  async function save(p: SettingsPatch): Promise<boolean> {
    const keys = Object.keys(p) as (keyof Settings)[];
    try {
      settings = await backend.setSettings(p);
      const next = { ...error };
      keys.forEach((k) => delete next[k]);
      error = next;
      ui.onChanged?.(p);
      return true;
    } catch (e) {
      error = { ...error, [keys[0]]: errText(e) };
      rev++;
      return false;
    }
  }

  return {
    get settings() {
      return settings;
    },
    get error() {
      return error;
    },
    get rev() {
      return rev;
    },
    load,
    save,
    /** Shows a change saved elsewhere (the status bar's toggles) without saving again. */
    mirror(p: SettingsPatch) {
      if (settings) settings = { ...settings, ...p };
    },
  };
}
