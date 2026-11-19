// The menu bar: menus as data, opened by click, closed by choosing, Escape or clicking elsewhere.
// While one menu is open, hovering another switches to it (as in Godot and native menu bars).
import { useEffect, useRef, useState } from "react";

/** One menu entry. */
export interface MenuItem {
  label: string;
  /** Shown on the right, e.g. "⌘S". */
  shortcut?: string;
  /** Default true. */
  enabled?: boolean;
  run: () => void;
}

/** A top-level menu. */
export interface Menu {
  label: string;
  items: (MenuItem | "separator")[];
}

const bar: React.CSSProperties = { display: "flex", gap: 2, position: "relative" };
const top = (open: boolean): React.CSSProperties => ({
  background: open ? "#3a3a3a" : "transparent",
  border: "none",
  color: "inherit",
  padding: "3px 8px",
  font: "inherit",
});
/** The look of a dropdown or context menu. */
export const popup: React.CSSProperties = {
  position: "absolute",
  top: "100%",
  zIndex: 20,
  minWidth: 220,
  background: "#2b2b2b",
  border: "1px solid #111",
  boxShadow: "0 4px 12px rgba(0,0,0,0.5)",
  padding: "4px 0",
};
const entry = (enabled: boolean): React.CSSProperties => ({
  display: "flex",
  justifyContent: "space-between",
  gap: 24,
  width: "100%",
  padding: "4px 12px",
  background: "transparent",
  border: "none",
  color: enabled ? "#ddd" : "#777",
  font: "inherit",
  textAlign: "left",
});

export function MenuBar({ menus }: { menus: Menu[] }) {
  const [open, setOpen] = useState<string>();
  const root = useRef<HTMLDivElement>(null);
  useDismiss(root, open !== undefined, () => setOpen(undefined));
  return (
    <div ref={root} role="menubar" style={bar}>
      {menus.map((menu) => (
        <div key={menu.label} style={{ position: "relative" }}>
          <button
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={open === menu.label}
            style={top(open === menu.label)}
            onClick={() => setOpen(open === menu.label ? undefined : menu.label)}
            onMouseEnter={() => open !== undefined && setOpen(menu.label)}
          >
            {menu.label}
          </button>
          {open === menu.label && (
            <MenuList label={menu.label} items={menu.items} style={popup} onDone={() => setOpen(undefined)} />
          )}
        </div>
      ))}
    </div>
  );
}

/** Calls `onClose` on Escape or a press outside `root`, while `active`. */
export function useDismiss(root: React.RefObject<HTMLElement | null>, active: boolean, onClose: () => void): void {
  const latest = useRef(onClose);
  useEffect(() => {
    latest.current = onClose;
  });
  useEffect(() => {
    if (!active) return;
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== "Escape") return;
      if (e instanceof MouseEvent && root.current?.contains(e.target as Node)) return;
      latest.current();
    };
    window.addEventListener("keydown", close);
    window.addEventListener("mousedown", close);
    return () => {
      window.removeEventListener("keydown", close);
      window.removeEventListener("mousedown", close);
    };
  }, [root, active]);
}

/** A list of menu entries (a menu bar popup or a context menu); choosing one calls `onDone`, then the item. */
export function MenuList({
  label,
  items,
  style,
  onDone,
}: {
  label: string;
  items: Menu["items"];
  style: React.CSSProperties;
  onDone: () => void;
}) {
  return (
    <div role="menu" aria-label={label} style={style}>
      {items.length === 0 && <div style={{ ...entry(false), fontStyle: "italic" }}>Nothing here yet</div>}
      {items.map((item, i) =>
        item === "separator" ? (
          <hr key={i} style={{ border: "none", borderTop: "1px solid #3c3c3c", margin: "4px 0" }} />
        ) : (
          <button
            key={item.label}
            role="menuitem"
            aria-disabled={item.enabled === false}
            style={entry(item.enabled !== false)}
            onClick={() => {
              if (item.enabled === false) return;
              onDone();
              item.run();
            }}
          >
            <span>{item.label}</span>
            {item.shortcut !== undefined && <span style={{ opacity: 0.6 }}>{item.shortcut}</span>}
          </button>
        ),
      )}
    </div>
  );
}
