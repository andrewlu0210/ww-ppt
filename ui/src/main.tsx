import { createRoot } from "react-dom/client";
import { App } from "./App";
import { Audience } from "./Audience";
import "./styles.css";

// The audience window (slides on the second display) loads the same page with `?audience`.
const audience = new URLSearchParams(window.location.search).has("audience");

// No StrictMode: its double-mounted effects would initialize WebGPU on the canvas twice.
createRoot(document.getElementById("root")!).render(audience ? <Audience /> : <App />);
