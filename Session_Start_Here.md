# Session Start Here

**Date:** 2026-06-07  
**Status:** ✅ UX proof real and working | ✅ First governance-core slice implemented  

## What's Proven

- **UX layer works**: Buddy surface, dock chrome, settings, UI bubbles—all live and functional
- **Backend scaffold**: Tauri overlay, vite dev, gateway communication layer in place
- **Repo hygiene**: `.gitignore` expanded; `.env.example`, `package-lock.json`, `Cargo.lock` all present; build scripts stable

## Governance Core Progress

The first deterministic governance slice is now in place:

- `MemoryPacket` schema
- `PurposePolicy` schema
- `MemoryGrader` logic
- `SafeContextFrame` buckets
- `PromptRenderer`
- `GradeReceipt` derivation trail
- Receipt summary view model for Nexus/Veritas wiring
- Nexus/Veritas panel data contracts for v0.2 scaffolding
- Browser-only Trust Workbench preview for Nexus/Veritas data
- Shared TrustWorkbenchPanel for browser and buddy surfaces
- Nexus buddy panel preview with retrieval grades, source list, and colored blocked counter
- Veritas buddy panel preview with warning counter, evidence-ready items, and receipt row expansion
- JSON Schemas for `MemoryPacket` and `PurposePolicy`
- Hermes mock memory demo
- CLI output for same-memory/different-purpose demo
- Governance trace reporter with `npm run demo:trace`
- Vitest coverage for required governance rules

## What's Still Missing (P0 Follow-Up)

The product proof still needs UI integration and deeper hardening:

- Trust Workbench action behavior: verify, open source, export receipt, use trusted only
- Receipt viewer with full derivation details
- UI tests for workbench section collapse and receipt expansion
- More edge-case policy tests as the primitive evolves

**Read the non-negotiable laws:** [AGENTS.md](AGENTS.md#non-negotiable-laws)

## Next Serious Milestone

**NOT more buddy chrome.**

**First core commit target:**  
`src/core/` governance primitives + test scaffold

See: [SPEC_MEMORY_GRADING.md](docs/SPEC_MEMORY_GRADING.md)

## Known Gaps (Still True)

- Core automated tests exist; UI/demo integration still needs coverage
- `tauri.conf.json` has `"csp": null` (needs review before trusted content)
- Tauri capabilities broad (document the threat model)
- Browser extension uses `innerHTML` (acceptable for now; guard when memory flows there)
- `BorderDock.tsx` and `src-tauri/src/lib.rs` need eventual refactor (not urgent)

## Key Documents

| Doc | Purpose |
|-----|---------|
| [AGENTS.md](AGENTS.md) | Project stance, translation rule, non-negotiable laws |
| [SPEC_MEMORY_GRADING.md](docs/SPEC_MEMORY_GRADING.md) | Governance primitive spec |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | System design overview |
| [FIX_LIST.md](docs/FIX_LIST.md) | Audit findings (partially stale) |

## How to Start Tomorrow

1. **Quick check**: `npm run dev` (browser preview) or `bash scripts/bb-start.sh` (full overlay)
2. **Review**: [SPEC_MEMORY_GRADING.md](docs/SPEC_MEMORY_GRADING.md) — understand the grading model
3. **Review**: `src/core/` implementation and `src/core/__tests__/`
4. **Demo**: Run `npm run demo:trace`, then open Nexus/Veritas buddy panels to inspect the shared Trust Workbench data
5. **Commit**: Keep future governance changes small and test-backed

## Translation Rule Reminder

```
Vision layer → metaphor allowed
Architecture layer → metaphor translated
API layer → metaphor removed
Test layer → metaphor forbidden
```

Use plain names in code: `MemoryPacket`, `MemoryGrader`, `GradeReceipt`, etc.  
Metaphor belongs in docs only.

---

**Trust decisions must be deterministic.**  
Every authorization override must produce a receipt.

Good luck! 🚀
