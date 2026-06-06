import { createRoot } from "react-dom/client";
import { BorderDock } from "../components/BorderDock";
import { initBbDiagnostics } from "./bbDiagnostics";
import "./styles.css";

initBbDiagnostics();

// StrictMode double-mount reconnects the desktop gateway and stresses WebKitGTK overlays.
createRoot(document.getElementById("root") as HTMLElement).render(<BorderDock />);
