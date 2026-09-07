import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SplitResult, AdbDevice, ApiError } from "../types";

const baseName = (p: string) => p.split(/[/\\]/).pop() ?? p;
const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

export function SplitPanel({ apkPath }: { apkPath: string }) {
  const [result, setResult] = useState<SplitResult | null>(null);
  const [devices, setDevices] = useState<AdbDevice[] | null>(null);
  const [selected, setSelected] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const run = async (label: string, fn: () => Promise<void>) => {
    setError(null);
    setBusy(label);
    try {
      await fn();
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(null);
    }
  };

  const extract = () =>
    run("extract", async () => {
      const r = await invoke<SplitResult>("extract_splits", { path: apkPath });
      setResult(r);
      setStatus(`${r.apks.length} APK parçası çıkarıldı`);
    });

  const refresh = () =>
    run("devices", async () => {
      const d = await invoke<AdbDevice[]>("list_adb_devices");
      setDevices(d);
      if (d.length > 0 && !selected) setSelected(d[0].serial);
    });

  const install = () =>
    run("install", async () => {
      if (!result) return;
      const out = await invoke<string>("install_splits", {
        serial: selected || null,
        apks: result.apks,
      });
      setStatus(out);
    });

  return (
    <div className="deploy-panel">
      <h3>XAPK / Split Birleştirici</h3>
      <p className="muted" style={{ margin: "0 0 14px" }}>
        XAPK/APKS içindeki parçaları çıkarır ve cihaza tek seferde
        (<span className="mono">adb install-multiple</span>) kurar.
      </p>

      <div className="deploy-actions">
        <button className="primary-btn" onClick={extract} disabled={busy !== null}>
          {busy === "extract" ? "Çıkarılıyor…" : "Split'leri çıkar"}
        </button>
        <button onClick={refresh} disabled={busy !== null}>
          Cihazları yenile
        </button>
        {devices && devices.length > 0 && (
          <select value={selected} onChange={(e) => setSelected(e.target.value)}>
            {devices.map((d) => (
              <option key={d.serial} value={d.serial}>
                {d.serial} ({d.state})
              </option>
            ))}
          </select>
        )}
        <button
          onClick={install}
          disabled={busy !== null || !result || result.apks.length === 0}
        >
          {busy === "install" ? "Kuruluyor…" : "install-multiple"}
        </button>
      </div>

      {status && <p className="patch-ok">{status}</p>}
      {error && <p className="error">Hata: {error}</p>}

      {result && (
        <ul className="split-list">
          {result.apks.map((a) => (
            <li key={a} className="mono">
              {baseName(a)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
