import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

// No StrictMode: its double-mounted effects would initialize WebGPU on the canvas twice.
createRoot(document.getElementById("root")!).render(<App />);
