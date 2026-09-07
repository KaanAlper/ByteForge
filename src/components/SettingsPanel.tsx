import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleCheck, CircleX, Check } from "lucide-react";
import type { ToolReport } from "../types";

const INSTALL_CMD =
  "yay -S android-apktool-bin android-sdk-build-tools android-tools jadx jdk-openjdk";
const PATH_CMD =
  "set BT (ls -d /opt/android-sdk/build-tools/*/ | sort -V | tail -1); fish_add_path $BT";

export function SettingsPanel() {
  const [report, setReport] = useState<ToolReport | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const load = () => {
    invoke<ToolReport>("check_tools").then(setReport).catch(console.error);
  };

  useEffect(() => {
    load();
  }, []);

  const copy = async (text: string, id: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(id);
      setTimeout(() => setCopied(null), 1500);
    } catch {
      /* clipboard yoksa yoksay */
    }
  };

  const missing = report?.tools.filter((t) => !t.available) ?? [];

  return (
    <div className="settings-panel">
      <div className="wb-header">
        <h3 style={{ margin: 0 }}>Araç Zinciri</h3>
        <div className="wb-actions">
          <button onClick={load}>yeniden denetle</button>
        </div>
      </div>

      <div className="tool-grid">
        {report?.tools.map((t) => (
          <div key={t.name} className={`tool-card ${t.available ? "ok" : "missing"}`}>
            <div className="tool-name">
              {t.available ? (
                <CircleCheck size={15} className="ic-ok" />
              ) : (
                <CircleX size={15} className="ic-bad" />
              )}{" "}
              {t.name}
            </div>
            <div className="tool-detail mono">{t.detail}</div>
          </div>
        ))}
      </div>

      {missing.length > 0 ? (
        <div className="install-help">
          <p className="field-label">{missing.length} araç eksik — CachyOS/Arch kurulumu:</p>
          <div className="cmd-box">
            <code>{INSTALL_CMD}</code>
            <button onClick={() => copy(INSTALL_CMD, "i")}>
              {copied === "i" ? <><Check size={13} /> kopyalandı</> : "kopyala"}
            </button>
          </div>
          <p className="field-label">Ardından build-tools'u PATH'e ekle (fish):</p>
          <div className="cmd-box">
            <code>{PATH_CMD}</code>
            <button onClick={() => copy(PATH_CMD, "p")}>
              {copied === "p" ? <><Check size={13} /> kopyalandı</> : "kopyala"}
            </button>
          </div>
          <p className="muted" style={{ fontSize: 12 }}>
            Kurulumdan sonra "yeniden denetle"ye basın. (Bu komutlar sudo/AUR
            etkileşimi ister — kendi terminalinizde çalıştırın.)
          </p>
        </div>
      ) : (
        report && (
          <p className="patch-ok">
            <CircleCheck size={15} className="ic-ok" /> Tüm araçlar hazır
          </p>
        )
      )}
    </div>
  );
}
