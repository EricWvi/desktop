import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { AppShell } from "@ora/features";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <AppShell />
  </StrictMode>,
);
