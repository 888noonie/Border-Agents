import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import "./buddy-surface.css";

export type BuddyMenuAction = {
  id: string;
  label: string;
  icon?: string;
  disabled?: boolean;
  danger?: boolean;
};

type BuddyActionMenuProps = {
  open: boolean;
  anchor: HTMLElement | null;
  actions: BuddyMenuAction[];
  boundaryRef?: RefObject<HTMLElement | null>;
  menuRef?: RefObject<HTMLDivElement | null>;
  onAction: (id: string) => void;
  onClose: () => void;
};

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

export function BuddyActionMenu({
  open,
  anchor,
  actions,
  boundaryRef,
  menuRef,
  onAction,
  onClose,
}: BuddyActionMenuProps) {
  const localMenuRef = useRef<HTMLDivElement>(null);
  const resolvedMenuRef = menuRef ?? localMenuRef;
  const [position, setPosition] = useState({ top: 0, left: 0 });
  const [visible, setVisible] = useState(false);

  useLayoutEffect(() => {
    if (!open || !anchor || !resolvedMenuRef.current) {
      setVisible(false);
      return;
    }

    setVisible(false);
    const frame = window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        const anchorRect = anchor.getBoundingClientRect();
        const boundaryRect = boundaryRef?.current?.getBoundingClientRect() ?? null;
        const menuRect = resolvedMenuRef.current?.getBoundingClientRect();
        const width = menuRect?.width ?? 180;
        const height = menuRect?.height ?? 120;
        const viewportMargin = 12;
        const minLeft = viewportMargin;
        const maxLeft = Math.max(viewportMargin, window.innerWidth - width - viewportMargin);
        const minTop = viewportMargin;
        const maxTop = Math.max(viewportMargin, window.innerHeight - height - viewportMargin);
        let left = anchorRect.right - width;
        let top = anchorRect.bottom + 8;

        if (boundaryRect) {
          const gap = 8;
          left = boundaryRect.right - width - gap;
          top = boundaryRect.bottom + gap;

          if (top + height > window.innerHeight - viewportMargin) {
            top = boundaryRect.bottom - height - gap;
          }

          if (top < boundaryRect.top + gap) {
            top = boundaryRect.top + gap;
          }
        } else if (top + height > window.innerHeight - viewportMargin) {
          top = anchorRect.top - height - 8;
        }

        setPosition({
          top: clamp(top, minTop, maxTop),
          left: clamp(left, minLeft, maxLeft),
        });
        setVisible(true);
      });
    });

    return () => window.cancelAnimationFrame(frame);
  }, [actions.length, anchor, boundaryRef, open, resolvedMenuRef]);

  useEffect(() => {
    if (!open) {
      return;
    }

    function handlePointerDown(event: PointerEvent) {
      const target = event.target as Node | null;
      if (resolvedMenuRef.current?.contains(target) || anchor?.contains(target)) {
        return;
      }
      onClose();
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }

    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [anchor, onClose, open, resolvedMenuRef]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="buddy-menu-layer"
      style={{
        top: position.top,
        left: position.left,
        visibility: visible ? "visible" : "hidden",
      }}
    >
      <div ref={resolvedMenuRef} className="buddy-menu" role="menu" id="buddy-action-menu">
        {actions.map((action) => (
          <button
            key={action.id}
            type="button"
            className={[
              "buddy-menu__action",
              action.danger ? "buddy-menu__action--danger" : "",
            ].join(" ")}
            role="menuitem"
            disabled={action.disabled}
            onClick={() => {
              onAction(action.id);
              onClose();
            }}
          >
            {action.icon ? <span className="buddy-menu__icon">{action.icon}</span> : null}
            <span>{action.label}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
