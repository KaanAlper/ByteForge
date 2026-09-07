import { useState, useEffect, useRef } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { Square, Play } from "lucide-react";
import type { AdbDevice, ResignResult, ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const MAX_LINES = 800;

export function DeployPanel({
  apkPath,
  packageName,
}: {
  apkPath: string;
  packageName: string | null;
}) {
  const [signedPath, setSignedPath] = useState<string | null>(null);
  const [devices, setDevices] = useState<AdbDevice[] | null>(null);
  const [selected, setSelected] = useState<string>("");
  const [log, setLog] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"sign" | "devices" | "install" | "launch" | null>(null);

  const [streaming, setStreaming] = useState(false);
  const [lines, setLines] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
  const logRef = useRef<HTMLPreElement>(null);

  const call = async (label: typeof busy, fn: () => Promise<void>) => {
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

  const resign = () =>
    call("sign", async () => {
      const r = await invoke<ResignResult>("resign_apk_with_mods", { path: apkPath });
      setSignedPath(r.signed_path);
      setLog(`${r.note}\n→ ${r.signed_path}`);
    });

  const refresh = () =>
    call("devices", async () => {
      const d = await invoke<AdbDevice[]>("list_adb_devices");
      setDevices(d);
      if (d.length > 0 && !selected) setSelected(d[0].serial);
    });

  const install = () =>
    call("install", async () => {
      const out = await invoke<string>("install_apk", {
        serial: selected || null,
        path: signedPath ?? apkPath,
      });
      setLog(out);
    });

  const launch = () =>
    call("launch", async () => {
      if (!packageName) return;
      const out = await invoke<string>("launch_app", {
        serial: selected || null,
        package: packageName,
      });
      setLog(out);
    });

  const startLogcat = async () => {
    setError(null);
    setLines([]);
    const channel = new Channel<string>();
    channel.onmessage = (line) => {
      setLines((prev) => {
        const next = prev.length >= MAX_LINES ? prev.slice(-MAX_LINES + 1) : prev;
        return [...next, line];
      });
    };
    try {
      await invoke("start_logcat", { serial: selected || null, onLine: channel });
      setStreaming(true);
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const stopLogcat = async () => {
    try {
      await invoke("stop_logcat");
    } catch {
      /* yoksay */
    }
    setStreaming(false);
  };

  // Bileşen kapanınca akışı durdur.
  useEffect(() => {
    return () => {
      invoke("stop_logcat").catch(() => {});
    };
  }, []);

  // Otomatik en alta kaydır.
  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [lines]);

  const shown = filter
    ? lines.filter((l) => l.toLowerCase().includes(filter.toLowerCase()))
    : lines;
  const noDevices = devices !== null && devices.length === 0;

  return (
    <div className="deploy-panel">
      <h3>Derle &amp; Dağıt</h3>
      <div className="deploy-actions">
        <button onClick={resign} disabled={busy !== null}>
          {busy === "sign" ? "Paketlenip imzalanıyor…" : "Modları dahil et + imzala"}
        </button>
        <button onClick={refresh} disabled={busy !== null}>
          {busy === "devices" ? "…" : "Cihazları yenile"}
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
        <button onClick={install} disabled={busy !== null || noDevices}>
          {busy === "install" ? "Yükleniyor…" : "Cihaza yükle"}
        </button>
        {packageName && (
          <button onClick={launch} disabled={busy !== null || noDevices}>
            {busy === "launch" ? "Başlatılıyor…" : "Cihazda başlat"}
          </button>
        )}
      </div>

      {signedPath && <p className="muted mono">{signedPath}</p>}
      {noDevices && <p className="muted">Bağlı cihaz yok (adb devices boş).</p>}
      {log && <pre className="deploy-log">{log}</pre>}
      {error && <p className="error">Hata: {error}</p>}

      <div className="logcat-section">
        <div className="logcat-bar">
          <h4>Canlı Logcat</h4>
          {streaming ? (
            <button className="logcat-stop" onClick={stopLogcat}>
              <Square size={13} /> durdur
            </button>
          ) : (
            <button className="logcat-start" onClick={startLogcat} disabled={noDevices}>
              <Play size={13} /> başlat
            </button>
          )}
          <input
            className="logcat-filter"
            placeholder="filtre — ör. FATAL, SIGSEGV, paket adı"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <div className="logcat-presets">
            {["FATAL", "SIGSEGV", "AndroidRuntime", "signature"].map((f) => (
              <button key={f} onClick={() => setFilter(f)}>
                {f}
              </button>
            ))}
            {filter && <button onClick={() => setFilter("")}>temizle</button>}
          </div>
        </div>
        <pre className="logcat-view" ref={logRef}>
          {streaming && shown.length === 0
            ? "akış bekleniyor…"
            : shown.slice(-400).join("\n") || (streaming ? "" : "başlatmak için başlat’a basın")}
        </pre>
      </div>
    </div>
  );
}
