import { createRoot } from "react-dom/client";
import { App } from "./App";
import { Audience } from "./Audience";
import { AboutWindow } from "./components/AboutWindow";
import { PreferencesWindow } from "./components/PreferencesWindow";
import "./styles.css";
import { initAppearance } from "./lib/appearance";

initAppearance();

// The audience window (slides on the second display) loads the same page with `?audience`; the About window uses `?about` and Preferences `?preferences`.
const params = new URLSearchParams(window.location.search);
const audience = params.has("audience");
const about = params.has("about");
const preferences = params.has("preferences");

// No StrictMode: its double-mounted effects would initialize WebGPU on the canvas twice.
createRoot(document.getElementById("root")!).render(preferences ? <PreferencesWindow /> : about ? <AboutWindow /> : audience ? <Audience /> : <App />);
