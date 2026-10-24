import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { Island } from "./Island";
import { Settings } from "./Settings";
import "./styles.css";

// Every window loads this bundle; the label decides what to render.
const label = getCurrentWindow().label;
document.documentElement.classList.toggle("island-window", label === "island");

const windows: Record<string, React.ReactNode> = { island: <Island />, settings: <Settings /> };

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{windows[label] ?? <App />}</React.StrictMode>,
);
