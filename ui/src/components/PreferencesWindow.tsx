import { useEffect, useState } from "react";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import { applyAppearance, loadAppearance, type Appearance } from "../lib/appearance";
import {
  announceAppearance,
  AUTOSAVE_CHOICES,
  autosaveSeconds,
  languagePref,
  setAutosaveSeconds,
  setLanguagePref,
  type LanguagePref,
} from "../lib/prefs";

const PREFS_LABEL = "preferences";

/** Opens the Preferences window, or brings it to the front when it is already open. */
export async function openPreferences() {
  const existing = await WebviewWindow.getByLabel(PREFS_LABEL);
  if (existing) {
    await existing.setFocus();
    return;
  }
  new WebviewWindow(PREFS_LABEL, {
    url: "index.html?preferences",
    title: t("prefs.title"),
    width: 440,
    height: 330,
    resizable: false,
    minimizable: false,
    maximizable: false,
    center: true,
  });
}

function everyLabel(seconds: number): string {
  if (seconds === 0) return t("prefs.autosaveOff");
  if (seconds < 60) return t("prefs.everySeconds", { count: seconds });
  return t("prefs.everyMinutes", { count: seconds / 60 });
}

/** Content of the Preferences window. Every change applies right away. */
export function PreferencesWindow() {
  const [appearance, setAppearance] = useState<Appearance>(loadAppearance);
  const [language, setLanguage] = useState<LanguagePref>(languagePref);
  const [autosave, setAutosave] = useState(autosaveSeconds);
  const close = () => void getCurrentWindow().close();

  // Also switches this window's own look and the native chrome (menus, dialogs).
  useEffect(() => applyAppearance(appearance, () => {}), [appearance]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || (e.metaKey && e.key === "w")) close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const startLanguage = useState(languagePref)[0];
  return (
    <div className="prefs-page">
      <div className="prefs-title">{t("prefs.title")}</div>
      <label className="prefs-row">
        {t("prefs.appearance")}
        <select
          value={appearance}
          onChange={(e) => {
            const v = e.target.value as Appearance;
            setAppearance(v);
            announceAppearance(v);
          }}
        >
          <option value="system">{t("ribbon.followSystem")}</option>
          <option value="light">{t("ribbon.light")}</option>
          <option value="dark">{t("ribbon.dark")}</option>
        </select>
      </label>
      <label className="prefs-row">
        {t("prefs.language")}
        <select
          value={language}
          onChange={(e) => {
            const v = e.target.value as LanguagePref;
            setLanguage(v);
            void setLanguagePref(v);
          }}
        >
          <option value="system">{t("prefs.languageSystem")}</option>
          <option value="zh-Hant">繁體中文</option>
          <option value="zh-Hans">简体中文</option>
          <option value="en">English</option>
        </select>
      </label>
      {language !== startLanguage && <div className="prefs-hint">{t("prefs.languageNote")}</div>}
      <label className="prefs-row">
        {t("prefs.autosave")}
        <select
          value={autosave}
          onChange={(e) => {
            const v = Number(e.target.value);
            setAutosave(v);
            setAutosaveSeconds(v);
          }}
        >
          {AUTOSAVE_CHOICES.map((s) => (
            <option key={s} value={s}>
              {everyLabel(s)}
            </option>
          ))}
        </select>
      </label>
      <div className="prefs-note">{t("prefs.note")}</div>
      <div className="prefs-buttons">
        <button className="primary" onClick={close} autoFocus>
          {t("prefs.done")}
        </button>
      </div>
    </div>
  );
}
