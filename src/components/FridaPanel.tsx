import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Copy, Check, Wand2 } from "lucide-react";
import type { ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

interface FridaScript {
  id: string;
  title: string;
  description: string;
  usage: string;
  script: string;
}

export function FridaPanel() {
  const [scripts, setScripts] = useState<FridaScript[]>([]);
  const [tracer, setTracer] = useState<FridaScript | null>(null);
  const [cls, setCls] = useState("");
  const [method, setMethod] = useState("");
  const [copied, setCopied] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<FridaScript[]>("frida_scripts").then(setScripts).catch((e) => setError(errMsg(e)));
  }, []);

  const copy = async (id: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(id);
      setTimeout(() => setCopied((c) => (c === id ? null : c)), 1200);
    } catch {
      /* pano yoksa geç */
    }
  };

  const genTracer = async () => {
    if (!cls.trim() || !method.trim()) return;
    try {
      setTracer(await invoke<FridaScript>("frida_tracer", { class: cls, method }));
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const card = (s: FridaScript) => (
    <div key={s.id} className="frida-card">
      <div className="frida-card-head">
        <span className="frida-title">{s.title}</span>
        <button className="frida-copy" onClick={() => copy(s.id, s.script)}>
          {copied === s.id ? <Check size={14} /> : <Copy size={14} />}
          {copied === s.id ? "kopyalandı" : "kopyala"}
        </button>
      </div>
      <p className="frida-desc muted">{s.description}</p>
      <code className="frida-usage mono">{s.usage}</code>
      <pre className="frida-script mono">{s.script}</pre>
    </div>
  );

  return (
    <div className="frida-panel">
      <h2 style={{ marginTop: 0 }}>Frida Script Kütüphanesi</h2>
      <p className="muted" style={{ marginTop: 0 }}>
        Hazır enstrümantasyon scriptleri. Cihazda <b>harici Frida</b> ile çalıştırılır
        (bu araç yalnızca kaynağı üretir; kök/USB hata ayıklama gerekir).
      </p>
      {error && <p className="error">Hata: {error}</p>}

      <div className="frida-tracer">
        <div className="manifest-head">
          <span className="field-label">Method Tracer üret</span>
        </div>
        <div className="inline-field">
          <input
            placeholder="sınıf — ör. com.example.Billing"
            value={cls}
            onChange={(e) => setCls(e.target.value)}
          />
          <input
            placeholder="metod — ör. isPremium"
            value={method}
            onChange={(e) => setMethod(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && genTracer()}
          />
          <button className="primary-btn" onClick={genTracer} disabled={!cls.trim() || !method.trim()}>
            <Wand2 size={15} /> üret
          </button>
        </div>
        {tracer && card(tracer)}
      </div>

      <div className="frida-list">{scripts.map(card)}</div>
    </div>
  );
}
