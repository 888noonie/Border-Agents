import { createRoot } from "react-dom/client";
import { DesktopBorderDock } from "../components/DesktopBorderDock";
import { BrowserBuddyDock } from "../components/BrowserBuddyDock";
import { initBbDiagnostics } from "./bbDiagnostics";
import "./styles.css";

initBbDiagnostics();

const isBrowser = !(window as any).__TAURI_INTERNALS__ && typeof (window as any).__TAURI__ === "undefined";

// StrictMode double-mount reconnects the desktop gateway and stresses WebKitGTK overlays.
createRoot(document.getElementById("root") as HTMLElement).render(
  isBrowser ? <BrowserBuddyDock /> : <DesktopBorderDock />
);
