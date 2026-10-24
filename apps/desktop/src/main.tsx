import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { Island } from "./Island";
import "./styles.css";

// Both windows load this bundle; the label decides what to render.
const isIsland = getCurrentWindow().label === "island";
document.documentElement.classList.toggle("island-window", isIsland);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isIsland ? <Island /> : <App />}</React.StrictMode>,
);
