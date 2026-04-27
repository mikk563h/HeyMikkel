import React, { useLayoutEffect } from "react";
import ReactDOM from "react-dom/client";
import { OverlayApp } from "./OverlayApp";
import "./overlay.css";

function EnsureTransparent() {
  useLayoutEffect(() => {
    const t = "transparent";
    document.documentElement.style.background = t;
    document.body.style.background = t;
  }, []);
  return null;
}

ReactDOM.createRoot(document.getElementById("overlay-root") as HTMLElement).render(
  <React.StrictMode>
    <EnsureTransparent />
    <OverlayApp />
  </React.StrictMode>,
);
