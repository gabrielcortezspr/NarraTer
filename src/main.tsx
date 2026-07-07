import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
// Self-hosted fonts (a Tauri app must work offline): Inter for the UI,
// JetBrains Mono for terminals/code — previously they only existed in font stacks.
import "@fontsource-variable/inter";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/700.css";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <App />
);
