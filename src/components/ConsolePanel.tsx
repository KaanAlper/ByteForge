import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Lightbulb } from "lucide-react";

/** Bilinen hata kalıplarına yönlendirici ipucu üretir. */
function errorHint(text: string): string | null {
  const t = text.toLowerCase();
  if (t.includes("resource") && (t.includes("not found") || t.includes("no resource")))
    return "Kaynak bulunamadı — genellikle res/values/public.xml çakışması. apktool ile temiz decode edip resources'ı kontrol edin.";
  if (t.includes("duplicate"))
    return "Yinelenen kaynak/sınıf çakışması — aynı dosyayı iki kez eklemiş olabilirsiniz.";
  if (t.includes("minsdkversion"))
    return "apksigner minSdk'yı okuyamadı — APK geçerli bir AndroidManifest.xml içermeli.";
  if (t.includes("no devices") || (t.includes("device") && t.includes("not found")))
    return "Cihaz yok — USB hata ayıklamayı açın ve Dağıt'ta 'Cihazları yenile'ye basın.";
  if (t.includes("signatures do not match") || t.includes("update_incompatible"))
    return "İmza uyuşmuyor — önce eski sürümü kaldırın: adb uninstall <paket>.";
  if (t.includes("install_failed"))
    return "Kurulum reddedildi — cihaz depolama/izin ya da imza sorunu olabilir.";
  if (t.includes("brut.") || t.includes("aapt"))
    return "apktool derleme hatası — çıktıdaki ilk 'Exception'/'error' satırını inceleyin.";
  return null;
}

const isErr = (l: string) => /error|fail|exception|hata|^!/i.test(l);

export function ConsolePanel() {
  const [lines, setLines] = useState<string[]>([]);
  const [auto, setAuto] = useState(true);
  const viewRef = useRef<HTMLDivElement>(null);

  const load = useCallback(async () => {
    try {
      setLines(await invoke<string[]>("read_console"));
    } catch {
      /* yoksay */
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    if (!auto) return;
    const id = setInterval(load, 1500);
    return () => clearInterval(id);
  }, [auto, load]);

  useEffect(() => {
    if (viewRef.current) viewRef.current.scrollTop = viewRef.current.scrollHeight;
  }, [lines]);

  const clear = async () => {
    await invoke("clear_console");
    setLines([]);
  };

  const lastError = [...lines].reverse().find(isErr);
  const hint = lastError ? errorHint(lastError) : null;

  return (
    <div className="console-panel">
      <div className="wb-header">
        <span className="muted">{lines.length} satır</span>
        <div className="wb-actions">
          <label className="console-auto">
            <input type="checkbox" checked={auto} onChange={(e) => setAuto(e.target.checked)} />
            otomatik
          </label>
          <button onClick={load}>yenile</button>
          <button onClick={clear}>temizle</button>
        </div>
      </div>

      <div className="console-view" ref={viewRef}>
        {lines.length === 0 ? (
          <span className="muted">
            Henüz komut yok. Smali decode/build, imzalama ve kurulum çıktıları burada toplanır.
          </span>
        ) : (
          lines.map((l, i) => (
            <div key={i} className={isErr(l) ? "cl-err" : l.startsWith("$") ? "cl-cmd" : ""}>
              {l}
            </div>
          ))
        )}
      </div>

      {hint && (
        <div className="error-hint">
          <Lightbulb size={14} /> <b>İpucu:</b> {hint}
        </div>
      )}
    </div>
  );
}
