import { useState, useEffect, useCallback } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { RefreshCw, Download, RotateCw, X, CircleCheck } from "lucide-react";

type Phase = "idle" | "checking" | "available" | "downloading" | "downloaded" | "uptodate" | "error";

/**
 * Uygulama içi güncelleme — GitHub Release'teki imzalı `latest.json`'ı kontrol
 * eder, paketi indirir (ilerleme çubuğu), sonra "Güncelleme indirildi —
 * yeniden başlat" popup'ı gösterir. Açılışta sessizce kontrol edip yalnızca
 * düğmede nokta rozet gösterir; kullanıcıyı bölmez.
 */
export function Updater() {
  const [version, setVersion] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [open, setOpen] = useState(false);
  const [progress, setProgress] = useState(0); // 0..1
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  const runCheck = useCallback(async (silent: boolean) => {
    setError(null);
    if (!silent) {
      setPhase("checking");
      setOpen(true);
    }
    try {
      const u = await check();
      if (u) {
        setUpdate(u);
        setPhase("available");
      } else {
        setUpdate(null);
        setPhase("uptodate");
        if (silent) setPhase("idle");
      }
    } catch (e) {
      if (!silent) {
        setError(String(e));
        setPhase("error");
      }
    }
  }, []);

  // Açılışta sessiz kontrol (ağ hatası olursa hiç bahsetme).
  useEffect(() => {
    const t = setTimeout(() => runCheck(true), 3000);
    return () => clearTimeout(t);
  }, [runCheck]);

  const install = async () => {
    if (!update) return;
    setPhase("downloading");
    setProgress(0);
    setError(null);
    let total = 0;
    let got = 0;
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") {
          total = ev.data.contentLength ?? 0;
        } else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          if (total > 0) setProgress(Math.min(1, got / total));
        } else if (ev.event === "Finished") {
          setProgress(1);
        }
      });
      setPhase("downloaded");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  };

  const hasUpdate = !!update && (phase === "available" || phase === "idle");

  return (
    <>
      <button
        className={`nav-item updater-btn ${hasUpdate ? "has-update" : ""}`}
        onClick={() => (hasUpdate ? (setOpen(true), setPhase("available")) : runCheck(false))}
        title={hasUpdate ? `Yeni sürüm: ${update?.version}` : "Güncellemeleri denetle"}
      >
        <RefreshCw className={`nav-icon ${phase === "checking" ? "spin" : ""}`} size={19} strokeWidth={1.85} />
        <span>Güncelle</span>
        {version && <span className="updater-ver mono">v{version}</span>}
        {hasUpdate && <span className="updater-dot" aria-label="güncelleme var" />}
      </button>

      {open && (
        <div className="updater-overlay" onClick={() => phase !== "downloading" && setOpen(false)}>
          <div className="updater-card" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
            <div className="updater-head">
              <h3>
                {phase === "checking" && "Güncellemeler denetleniyor…"}
                {phase === "available" && `ByteForge ${update?.version} hazır`}
                {phase === "downloading" && "Güncelleme indiriliyor…"}
                {phase === "downloaded" && "Güncelleme indirildi"}
                {phase === "uptodate" && "Uygulama güncel"}
                {phase === "error" && "Güncelleme başarısız"}
              </h3>
              {phase !== "downloading" && (
                <button className="updater-close" onClick={() => setOpen(false)} title="kapat">
                  <X size={15} />
                </button>
              )}
            </div>

            {phase === "available" && (
              <>
                <p className="muted">
                  Şu an <b className="mono">v{version}</b> — yeni sürüm <b className="mono">v{update?.version}</b>
                  {update?.date && <> · {update.date.slice(0, 10)}</>}
                </p>
                {update?.body && <pre className="updater-notes">{update.body}</pre>}
                <div className="updater-actions">
                  <button className="primary-btn" onClick={install}>
                    <Download size={15} /> İndir ve kur
                  </button>
                  <button onClick={() => setOpen(false)}>Sonra</button>
                </div>
              </>
            )}

            {phase === "downloading" && (
              <>
                <div className="updater-bar">
                  <div className="updater-bar-fill" style={{ width: `${Math.round(progress * 100)}%` }} />
                </div>
                <p className="muted mono">{Math.round(progress * 100)}%</p>
              </>
            )}

            {phase === "downloaded" && (
              <>
                <p className="muted">
                  <CircleCheck size={14} className="ic-ok" /> Yeni sürüm kuruldu. Uygulanması için
                  ByteForge'un yeniden başlatılması gerekiyor.
                </p>
                <div className="updater-actions">
                  <button className="primary-btn" onClick={() => relaunch()}>
                    <RotateCw size={15} /> Yeniden başlat
                  </button>
                  <button onClick={() => setOpen(false)}>Daha sonra</button>
                </div>
              </>
            )}

            {phase === "uptodate" && (
              <p className="muted">
                <CircleCheck size={14} className="ic-ok" /> <b className="mono">v{version}</b> en son sürüm.
              </p>
            )}

            {phase === "error" && (
              <>
                <p className="error">{error}</p>
                <div className="updater-actions">
                  <button onClick={() => runCheck(false)}>Tekrar dene</button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </>
  );
}
