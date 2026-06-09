# Playwright E2E Handover for AI Coding Agents

**Target audience:** Codex, Claude Code, Cursor Composer, other Grok instances, or any AI coding assistant working in this repo.

This document is the recommended handover for using the Playwright setup to develop the Border Agents UI.

## 1. One-Sentence Purpose

Playwright exists here so AI agents can **reliably explore, drive, and assert on the visible governance surfaces** (the "borders") that this product is built to make inspectable.

## 2. Quick Start (Copy-Paste These)

From the repo root:

```bash
# Best interactive first action when you need to understand or change the UI
npm run test:e2e:codegen

# Run the current suite (auto-starts the dev server)
npm run test:e2e

# Visual debugging / step through
npm run test:e2e:ui

# See the browser live
npm run test:e2e:headed

# After a failure, open the best artifact
npx playwright show-trace test-results/...
# or open playwright-report/index.html
```

The `webServer` config in `playwright.config.ts` means you almost never need to manually run `npm run dev` first.

## 3. Recommended Exploration Protocol

When you are asked to work on any UI, dock, buddy, workbench, receipt, or governance surface:

1. **Use codegen when interaction details are unclear** and an interactive display is available.
   ```bash
   npm run test:e2e:codegen
   ```
   - Click the things you need to understand.
   - Copy useful locators and flows directly into tests or scripts.
   - In headless, remote, or non-interactive sessions, inspect existing tests and use targeted Playwright runs instead.

2. Run the existing E2E suite to establish a baseline:
   ```bash
   npm run test:e2e
   ```

3. Make your change.

4. Re-run `npm run test:e2e` (or targeted file) + inspect artifacts on failure.

5. If behavior is surprising, use `--ui` or traces immediately.

Avoid guessing at selectors. Prefer codegen, existing tests, traces, or role/text locators grounded in the rendered UI.

## 4. How to Write Tests That Actually Matter (Project Rules)

This project follows strict guidance from [AGENTS.md](../AGENTS.md).

**Every change to the frontend should be able to answer these four questions** (the PR checklist):

1. What border does this change make visible?
2. What trust decision does it make inspectable?
3. What receipt does it produce?
4. What tests prove it?

### Good test patterns here

- Name describes the **governance surface or decision**, not the widget.
  - Good: `exposes memory grading signals in the dock for summarize_history purpose`
  - Bad: `clicks the buddy bubble`

- Assert on **visible evidence of grading / purpose / receipts / safe context**.
  - Current live signals (as of this handover): buddies actively announce "Memory graded! Trusted pieces", "1 of 5 sources passed receipt checks", etc.

- Prefer `getByRole`, `getByText`, and `first()` to avoid strict mode violations (we have many buddy bubbles).

- Keep tests resilient to early UI iteration but still prove the governance layer is rendered.

- The translation rule applies to tests:
  - Use concrete, observable things only (no "border patrol" language in the test code itself).

### Current test files (expand these)

- `e2e/smoke.spec.ts` — basic boot + dock presence.
- `e2e/governance-surfaces.spec.ts` — asserts that memory-grading and receipt signals are visible in the UI layer.

## 5. Current Observed UI State (Very Useful)

When the Vite dev server is running at `http://127.0.0.1:1420` (browser preview mode), the app currently renders:

- A prominent **Border Buddies dock** (`getByRole('main', { name: /Border Buddies dock/i })`)
- Multiple interactive **buddy bubbles** (role=button) with live governance content:
  - "Memory graded! Trusted pieces ready?"
  - "1 of 5 sources passed receipt checks."
  - "Hermes gateway ready (...)"
- The `#root` container is always present.
- The UI is in "unified + preview" mode (`border-dock--preview`, etc.).

These signals are already exercising the core `MemoryGrader` / `GradeReceipt` concepts visually. Future tests should expand on purpose-specific rendering, limited vs trusted visibility, receipt detail levels, etc.

Use codegen to discover more stable attributes as the UI evolves.

## 6. Debugging Playbook for Agents

When a test fails or the UI behaves unexpectedly:

1. Re-run the specific test with trace:
   ```bash
   npx playwright test e2e/smoke.spec.ts --trace on
   ```

2. Open the trace:
   ```bash
   npx playwright show-trace test-results/...
   ```

3. For live stepping + DOM inspection:
   ```bash
   npm run test:e2e:ui
   ```

4. Headed mode + slowMo for very tricky timing:
   ```bash
   npx playwright test --headed --slow-mo=200
   ```

Artifacts are automatically captured on first retry (traces, screenshots, video).

The HTML report is also generated: `playwright-report/index.html`.

## 7. Key Config Details

See [playwright.config.ts](../playwright.config.ts). Highlights for agents:

- `baseURL`: `http://127.0.0.1:1420`
- `webServer`: auto-starts `npm run dev`, reuses in non-CI
- `trace: 'on-first-retry'`, screenshot + video on failure
- Single project: Chromium (best match for Tauri webview)
- Timeouts are slightly generous for this Linux environment

You can temporarily edit the config for debugging (e.g. `headless: false`), but commit only intentional changes.

## 8. Tauri vs Web Layer Reality

- These tests drive the **web frontend** served by Vite.
- This is the exact surface that becomes the Tauri webview on desktop.
- Full native Tauri E2E (launching the built app binary) is **not** in scope for v0.1.
- Browser preview mode (`npm run dev`) is the primary development + test target.

## 9. Handoff Checklist (When You Finish Work)

Before you consider a UI change complete, the receiving agent (or human) should be able to say yes to:

- [ ] Used codegen, traces, or existing tests to ground UI selectors
- [ ] All E2E tests are green (`npm run test:e2e`)
- [ ] New or updated tests assert on a visible trust/governance decision
- [ ] On any flaky behavior, traces/screenshots were inspected and the cause addressed
- [ ] The four AGENTS.md PR questions can be answered clearly

## 10. Context You Can Paste to Another Agent

```text
You are working in the Border-Agents repo.

We have full Playwright E2E set up for the web UI layer.

Read the handover document first:
docs/PLAYWRIGHT_FOR_AI_AGENTS.md

Key commands:
- npm run test:e2e:codegen   (best interactive exploration tool)
- npm run test:e2e
- npm run test:e2e:ui

All E2E tests live in the e2e/ folder.

Follow the project AGENTS.md strictly, especially the four PR questions and the translation rule (tests must be concrete).

Current UI has live memory-grading buddy bubbles. Use codegen or traces to explore.
```

---

Use this document as the shared frontend testing handover. Agents should read it before changing the UI when time and context allow.

If you improve the setup, the tests, or this document itself, update this file and mention the improvement in your handoff.

Welcome to the border. Make the trust visible.
