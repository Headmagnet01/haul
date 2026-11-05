import { expect, type Page } from '@playwright/test';

/** Dispatches a Ctrl+V-style paste of `text` on the page. */
export async function paste(page: Page, text: string) {
  await page.evaluate((t) => {
    const dt = new DataTransfer();
    dt.setData('text/plain', t);
    document.body.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
  }, text);
}

const nav = (page: Page, name: RegExp) => page.getByRole('navigation', { name: 'Views' }).getByRole('button', { name });

/** In the Collector: waits for the checks, starts every online link and returns to the Queue. */
export async function startAllInCollector(page: Page) {
  await nav(page, /Collector/).click();
  const list = page.locator('[aria-label="Collector links"]');
  await expect(list.getByText('Checking…')).toHaveCount(0);
  await page.getByRole('button', { name: 'Start all online' }).click();
  await expect(page.getByRole('status')).toContainText('Started');
  await nav(page, /Queue/).click();
}

/** Pastes links, which land in the Collector, then starts them so they appear in the Queue. */
export async function addViaCollector(page: Page, text: string) {
  await paste(page, text);
  await expect(page.getByRole('status')).toContainText('to the Collector');
  await startAllInCollector(page);
}
