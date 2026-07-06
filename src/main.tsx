import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
// Fontes self-hosted (app Tauri precisa funcionar offline): Inter para a UI,
// JetBrains Mono para terminais/código — antes só existiam nos font stacks.
import "@fontsource-variable/inter";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/700.css";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <App />
);
