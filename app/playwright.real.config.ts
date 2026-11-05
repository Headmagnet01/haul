import { defineConfig } from '@playwright/test';

// Drives the real app (`npx tauri dev` started with WebView2's DevTools port 9222). No web server.
export default defineConfig({
  testDir: 'e2e-real',
  timeout: 120_000,
  workers: 1,
  reporter: 'list',
});
