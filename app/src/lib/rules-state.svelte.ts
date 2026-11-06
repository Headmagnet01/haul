import type { Backend } from './backend';
import { errText } from './postproc';
import { moveItem } from './rules';
import type { Category, PostProcSettings, Rule, RulesConfig } from './types';

export type RulesState = ReturnType<typeof createRulesState>;

/** The Rules view's data: switches, categories, rules and saved-password ids. */
export function createRulesState(backend: Backend, ui: { showToast: (text: string) => void }) {
  let config = $state<RulesConfig | null>(null);

  async function load() {
    try {
      config = await backend.rulesConfig();
    } catch (e) {
      ui.showToast(errText(e));
    }
  }

  async function run(f: () => Promise<unknown>, done?: string): Promise<boolean> {
    try {
      await f();
      if (done) ui.showToast(done);
      return true;
    } catch (e) {
      ui.showToast(errText(e));
      return false;
    } finally {
      await load();
    }
  }

  return {
    get config() {
      return config;
    },
    load,
    setSetting(key: keyof PostProcSettings, on: boolean) {
      if (!config) return Promise.resolve(false);
      const s = { ...config.settings, [key]: on };
      config.settings = s; // show it at once; a failure reloads the saved value
      return run(() => backend.setPostprocSettings(s));
    },
    saveCategories: (c: Category[]) => run(() => backend.setCategories(c), 'Rules saved'),
    saveRules: (r: Rule[]) => run(() => backend.setRules(r), 'Rules saved'),
    moveRule(i: number, by: -1 | 1) {
      if (!config) return Promise.resolve(false);
      return run(() => backend.setRules(moveItem($state.snapshot(config!.rules) as Rule[], i, by)));
    },
    addPassword: (pw: string) => run(() => backend.addPassword(pw), 'Password saved'),
    removePassword: (id: number) => run(() => backend.removePassword(id)),
    movePassword(i: number, by: -1 | 1) {
      if (!config) return Promise.resolve(false);
      return run(() => backend.orderPasswords(moveItem([...config!.passwords], i, by)));
    },
  };
}
