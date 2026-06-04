import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BorderDock } from "../components/BorderDock";
import "./styles.css";

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <BorderDock />
  </StrictMode>,
);
