import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import { APP_NAME } from "../lib/backend";
import icon from "../assets/app-icon.png";

const ABOUT_LABEL = "about";

/** Opens the About window, or brings it to the front when it is already open. */
export async function openAbout() {
  const existing = await WebviewWindow.getByLabel(ABOUT_LABEL);
  if (existing) {
    await existing.setFocus();
    return;
  }
  new WebviewWindow(ABOUT_LABEL, {
    url: "index.html?about",
    title: t("about.title", { name: APP_NAME }),
    width: 420,
    height: 410,
    resizable: false,
    minimizable: false,
    maximizable: false,
    center: true,
  });
}

/** Content of the About window: app icon, name, version, a one-line description and the maker. */
export function AboutWindow() {
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion, () => {});
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || (e.metaKey && e.key === "w")) void getCurrentWindow().close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  return (
    <div className="about-page">
      <img className="about-icon" src={icon} alt="" draggable={false} />
      <div className="about-name">{APP_NAME}</div>
      <div className="about-version">{t("about.version", { version })}</div>
      <div className="about-desc">{t("about.description")}</div>
      <div className="about-maker">{t("about.maker")}</div>
    </div>
  );
}
