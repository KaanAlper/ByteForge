import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Anchor, Copy, CheckCircle2, Code2, Rocket, Settings2 } from "lucide-react";

export function PayloadGenerator() {
  const [os, setOs] = useState("linux");
  const [domain, setDomain] = useState("lisans.adobe.com");
  const [response, setResponse] = useState('{"status": "active"}');
  
  const [exePath, setExePath] = useState("");
  const [generatedCode, setGeneratedCode] = useState("");
  const [copied, setCopied] = useState(false);
  const [isInjecting, setIsInjecting] = useState(false);

  const generateManual = async () => {
    try {
      const code = await invoke<string>("generate_payload", { os, domain, response });
      setGeneratedCode(code);
      setCopied(false);
    } catch (err) {
      alert(String(err));
    }
  };
  
  const autoInject = async () => {
    if (!exePath.trim()) {
      alert("Lütfen kancanın enjekte edileceği uygulamanın tam yolunu girin.");
      return;
    }
    
    setIsInjecting(true);
    try {
      // 1. Kodu Üret
      const code = await invoke<string>("generate_payload", { os, domain, response });
      setGeneratedCode(code);
      
      // 2. Kodu Derle (.so)
      const hookPath = await invoke<string>("compile_payload", { os, code });
      
      // 3. Oyunu LD_PRELOAD ile başlat
      await invoke("launch_with_hook", { exePath, hookPath });
      
      alert("Başarılı! Hedef uygulama kırılmış (Kancalanmış) şekilde arka planda başlatıldı.");
    } catch (err) {
      alert("Hata: " + String(err));
    } finally {
      setIsInjecting(false);
    }
  };

  const copyToClipboard = () => {
    navigator.clipboard.writeText(generatedCode);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="ms-root" style={{ display: "flex", flexDirection: "column", height: "100%", padding: 20, overflowY: "auto" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 20 }}>
        <Anchor size={24} style={{ color: "#ff4d4d" }} />
        <h2 style={{ margin: 0 }}>Kanca & Otomatik Başlatıcı (Payload Generator)</h2>
      </div>
      
      <p style={{ color: "var(--muted)", marginBottom: 30, maxWidth: 800 }}>
        Uygulama dosyasına (EXE/SO) dokunmaya izin vermeyen <b>zırhlı programlar</b> için kalıcı "In-Memory Hook" dosyası oluşturur ve otomatik olarak hedef uygulamaya enjekte ederek çalıştırır.
      </p>

      <div style={{ background: "#1a2030", padding: 20, borderRadius: 8, display: "flex", flexDirection: "column", gap: 16, maxWidth: 800 }}>
        <div style={{ display: "flex", gap: 16 }}>
          <div style={{ flex: 1 }}>
            <label style={{ display: "block", marginBottom: 6, fontSize: 13, color: "var(--muted)" }}>Hedef İşletim Sistemi</label>
            <select 
              className="ms-input"
              value={os}
              onChange={e => setOs(e.target.value)}
              style={{ width: "100%", padding: 8, background: "#0a0d14", border: "1px solid #2a3344", color: "#fff", borderRadius: 4 }}
            >
              <option value="linux">Linux (.so / LD_PRELOAD Otomasyonu)</option>
              <option value="windows">Windows (.dll / Manuel API Hooking)</option>
            </select>
          </div>
          <div style={{ flex: 1 }}>
            <label style={{ display: "block", marginBottom: 6, fontSize: 13, color: "var(--muted)" }}>İzlenecek Adres (Opsiyonel)</label>
            <input 
              className="ms-input" 
              value={domain} 
              onChange={e => setDomain(e.target.value)} 
              style={{ width: "100%", padding: 8, background: "#0a0d14", border: "1px solid #2a3344", color: "#fff", borderRadius: 4 }} 
            />
          </div>
        </div>

        <div>
          <label style={{ display: "block", marginBottom: 6, fontSize: 13, color: "var(--muted)" }}>Sahte Yanıt (Payload)</label>
          <textarea 
            value={response} 
            onChange={e => setResponse(e.target.value)} 
            style={{ width: "100%", height: 80, padding: 8, background: "#0a0d14", border: "1px solid #2a3344", color: "#ff7a3c", fontFamily: "var(--mono)", borderRadius: 4, resize: "vertical" }} 
          />
        </div>

        {os === "linux" && (
          <div style={{ padding: 15, background: "rgba(77, 255, 136, 0.1)", borderRadius: 6, border: "1px solid rgba(77, 255, 136, 0.3)" }}>
            <label style={{ display: "block", marginBottom: 6, fontSize: 13, color: "#4dff88", fontWeight: "bold" }}>TAM OTOMASYON (Otomatik Derle & Başlat)</label>
            <div style={{ display: "flex", gap: 10 }}>
              <input 
                placeholder="Örn: /usr/bin/gedit veya /home/user/oyun" 
                value={exePath}
                onChange={e => setExePath(e.target.value)}
                style={{ flex: 1, padding: 8, background: "#0a0d14", border: "1px solid #2a3344", color: "#fff", borderRadius: 4 }} 
              />
              <button 
                onClick={autoInject} 
                disabled={isInjecting}
                style={{ 
                  display: "flex", alignItems: "center", gap: 8, padding: "8px 16px", borderRadius: 4, border: "none", cursor: "pointer",
                  background: isInjecting ? "#555" : "#4dff88", color: "#000", fontWeight: "bold"
                }}
              >
                {isInjecting ? <Settings2 size={16} className="spin" /> : <Rocket size={16} />} 
                {isInjecting ? "Kanca Enjekte Ediliyor..." : "Kır ve Başlat"}
              </button>
            </div>
          </div>
        )}

        <button 
          onClick={generateManual} 
          style={{ 
            display: "flex", alignItems: "center", justifyContent: "center", gap: 8, padding: "10px 16px", borderRadius: 6, border: "none", cursor: "pointer",
            background: "#2a3344", color: "#fff", fontWeight: "bold", marginTop: 10
          }}
        >
          <Code2 size={16} /> Sadece Kodu Göster (Manuel Mod)
        </button>
      </div>

      {generatedCode && (
        <div style={{ marginTop: 30, maxWidth: 800 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", borderBottom: "1px solid #2a3344", paddingBottom: 10, marginBottom: 15 }}>
            <h3 style={{ margin: 0 }}>Üretilen Kanca Kodu</h3>
            <button 
              onClick={copyToClipboard}
              className="ms-btn-secondary"
              style={{ padding: "6px 12px", background: copied ? "rgba(77, 255, 136, 0.2)" : undefined, color: copied ? "#4dff88" : undefined }}
            >
              {copied ? <CheckCircle2 size={14} /> : <Copy size={14} />} {copied ? "Kopyalandı!" : "Kodu Kopyala"}
            </button>
          </div>
          <pre style={{ 
            background: "#0a0d14", padding: 15, borderRadius: 8, overflowX: "auto", border: "1px solid #2a3344",
            color: "#a8b2c8", fontFamily: "var(--mono)", fontSize: 13, lineHeight: 1.5
          }}>
            {generatedCode}
          </pre>
        </div>
      )}
    </div>
  );
}
