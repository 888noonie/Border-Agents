# Architecture

Border Agents is a visible governance surface for AI trust boundaries.

The v0.1 architecture is intentionally small:

```text
mock/vector results
→ MemoryGrader
→ SafeContextFrame
→ PromptRenderer / UI panels
```

The goal is to prove that retrieved memory can be preserved, graded by purpose, rendered safely, and audited.

---

## Core concepts

### MemoryPacket

A packet attached beside a retrieved memory chunk. It describes provenance, authority, sensitivity, permissions, validity, labels, and derivation.

### PurposePolicy

A deterministic authorization contract. A purpose is not a free-form label. It resolves to policy rules that decide which grades may enter prompt context and what permissions are required.

### MemoryGrader

Evaluates retrieved chunks against the active PurposePolicy and assigns grades.

### SafeContextFrame

A preserved ledger of retrieved chunks grouped by authorization grade:

```json
{
  "trusted": [],
  "limited": [],
  "reference_only": [],
  "blocked": [],
  "quarantined": []
}
```

### PromptRenderer

Converts a SafeContextFrame into prompt-safe context according to a render mode:

- `clean`: trusted context only
- `annotated`: trusted plus constrained limited context
- `strict`: trusted only, plus refusal/verification instruction when required context is unavailable

### GradeReceipt

Machine-readable evidence explaining why a chunk received a grade.

---

## Data flow

```text
Query
→ Vector search or mocked retrieved results
→ Retrieved chunks with MemoryPackets
→ PurposePolicy resolution
→ MemoryGrader
→ SafeContextFrame
→ PromptRenderer / Trust Workbench / audit log
```

---

## UI mapping

- **Nexus** shows retrieved memory and grade distribution.
- **Veritas** shows claim and evidence status.
- **Forge** later handles tool/code/action approval.
- **Nova** later handles artifact packaging.

v0.1 should wire Nexus and Veritas only.

---

## Design constraints

- Preserve retrieval results. Never silently erase relevant context.
- Grade authorization deterministically.
- Keep LLMs out of enforcement decisions.
- Emit receipts for every grade.
- Make blocked and limited states visible to developers/users through inspectable frames.
