import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App.tsx";
import "./index.css";
import { installAccessGate } from "./access/accessGate.ts";
import { installLayoutAttribute } from "./hooks/useCompactLayout.ts";

installAccessGate();
installLayoutAttribute();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
