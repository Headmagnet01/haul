import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'e2e',
  timeout: 30_000,
  fullyParallel: true,
  reporter: 'list',
  webServer: { command: 'npm run dev:mock', port: 5174, reuseExistingServer: true },
  use: { baseURL: 'http://localhost:5174', channel: 'msedge' },
  projects: [
    { name: 'light', use: { colorScheme: 'light' } },
    { name: 'dark', use: { colorScheme: 'dark' } },
  ],
});
