# E2E with Playwright for Border Agents

**For Codex, Claude Code, Cursor, or other AI agents:**  
→ Read the full dedicated handover first: [docs/PLAYWRIGHT_FOR_AI_AGENTS.md](../docs/PLAYWRIGHT_FOR_AI_AGENTS.md)

This setup exists so **AI agents and humans** can reliably drive and inspect the visible governance surfaces of the app.

## Why this exists (per project goals)

- The product makes trust boundaries inspectable.
- The web layer (Vite dev server → Tauri webview) is the primary place those boundaries become visible to users.
- Agents need easy, reproducible ways to:
  - Explore the current UI (`codegen`)
  - Write tests that prove a border is visible or a trust decision is rendered
  - Get artifacts (traces, screenshots, video) when something is wrong
  - Run against the live dev server without manual steps

## Quick commands (from repo root)

```bash
# Run all E2E tests (auto-starts `npm run dev` if the server isn't up)
npm run test:e2e

# Interactive UI mode — excellent for agents to explore live
npm run test:e2e:ui

# Headed (see the browser) for local debugging
npm run test:e2e:headed

# Generate locators / flows instantly (best agent exploration tool)
npm run test:e2e:codegen
# or directly:
npx playwright codegen http://127.0.0.1:1420

# View a trace after a failure
npx playwright show-trace test-results/...
```

## Agent workflow tips

See the much more detailed protocol and "paste this to another agent" block in `docs/PLAYWRIGHT_FOR_AI_AGENTS.md`. The notes below are the short version.

1. **Exploration first**
   - Run `npm run test:e2e:codegen`
   - Click around the dock, buddies, Trust Workbench preview, etc.
   - Copy the generated code into a new `.spec.ts` file.

2. **When a test is flaky or surprising**
   - Re-run with `--trace on` or let the config's `on-first-retry` do it.
   - Open the HTML report: `playwright-report/index.html`
   - Use `npx playwright show-trace ...`

3. **Adding tests that matter**
   - Focus on **visible governance surfaces**:
     - Is the border/dock rendering the right state?
     - Are grades (`trusted`, `limited`, `blocked`...) shown or hidden correctly for a purpose?
     - Can a user/agent see the receipt trail?
     - Does strict/annotated/clean render mode change what is in the prompt context preview?
   - Name tests after the border or decision being made visible (not just "clicks button").

4. **Tauri vs browser preview**
   - These tests run against the Vite web layer (`http://127.0.0.1:1420`).
   - This is the same surface that becomes the Tauri webview.
   - Full desktop Tauri E2E (launching the .deb/.app binary and driving the native window) is out of scope for v0.1 unless a specific need arises.

5. **WebServer magic**
   - The `playwright.config.ts` has a `webServer` block.
   - `npx playwright test` will start `npm run dev` for you and wait for port 1420.
   - In CI or when you want isolation, the server starts fresh.

## Current test files

- `smoke.spec.ts` — basic boot + structural presence of the dock/workbench area.
- `governance-surfaces.spec.ts` — higher-level assertions about the trust/governance UI being reachable.

Expand these as the Trust Workbench, receipt panels, purpose selectors, etc. become more functional.

## Config highlights (playwright.config.ts)

- `baseURL`: 127.0.0.1:1420
- Auto `webServer`
- Trace + screenshot + video on failure
- Chromium primary (matches Tauri webview on most platforms)
- HTML + list reporters

## System note

On this Linux environment a system Chrome exists and Playwright installed its own Chromium (Chrome for Testing). Tests default to the Playwright-managed browser for reproducibility.

---

Follow the same translation rule as the rest of the project:
- Vision: "border", "patrol"
- Architecture: boundaries, frames, receipts
- Tests & code: plain selectors, concrete assertions about rendered state and user/agent-visible decisions.
