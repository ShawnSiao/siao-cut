import { defineConfig } from "@playwright/test";
const port = Number(process.env.SIAOCUT_E2E_PORT ?? 4313);

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  use: { baseURL: `http://127.0.0.1:${port}`, locale: "zh-CN", trace: "retain-on-failure" },
  webServer: {
    command: `node node_modules/vite/bin/vite.js --host 127.0.0.1 --port ${port}`,
    url: `http://127.0.0.1:${port}`,
    reuseExistingServer: !process.env.SIAOCUT_E2E_PORT,
    timeout: 30_000,
  },
});
