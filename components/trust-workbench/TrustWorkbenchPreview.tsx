import { useState } from "react";
import { TrustWorkbenchPanel } from "./TrustWorkbenchPanel";
import "./TrustWorkbenchPreview.css";

export function TrustWorkbenchPreview() {
  const [open, setOpen] = useState(true);

  if (!open) {
    return (
      <button
        className="trust-preview-launcher"
        type="button"
        onClick={() => setOpen(true)}
        aria-label="Open Trust Workbench preview"
      >
        Trust Workbench
      </button>
    );
  }

  return (
    <aside className="trust-preview" aria-label="Trust Workbench preview">
      <div className="trust-preview__floating-actions">
        <button
          className="trust-preview__icon-button"
          type="button"
          onClick={() => setOpen(false)}
          aria-label="Minimize Trust Workbench preview"
          title="Minimize"
        >
          -
        </button>
      </div>
      <TrustWorkbenchPanel />
    </aside>
  );
}
