import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Server, Play, Square, AlertTriangle, ShieldCheck } from "lucide-react";

interface MockRequestInfo {
  ip: string;
  path: string;
  method: string;
  body_preview: string;
}

export function MockServerPanel() {
  const [domain, setDomain] = useState("lisans.adobe.com");
  const [port, setPort] = useState(80);
  const [response, setResponse] = useState('{"status": "active"}');
  const [active, setActive] = useState(false);
  const [requests, setRequests] = useState<MockRequestInfo[]>([]);

  useEffect(() => {
    let unlistenReq: () => void;
    let unlistenErr: () => void;
    
    listen<MockRequestInfo>("mock-request", (e) => {
      setRequests((prev) => [e.payload, ...prev].slice(0, 50));
    }).then(u => unlistenReq = u);
    
    listen<string>("mock-error", (e) => {
      alert("Sunucu Hatası: " + e.payload);
      setActive(false);
    }).then(u => unlistenErr = u);
    
    return () => {
      if (unlistenReq) unlistenReq();
      if (unlistenErr) unlistenErr();
      invoke("stop_mock_server");
    };
  }, []);

  const toggle = async () => {
    if (active) {
      await invoke("stop_mock_server");
      setActive(false);
    } else {
      try {
        await invoke("start_mock_server", { domain, port: Number(port), response });
        setActive(true);
        setRequests([]); // clear previous
      } catch (err) {
        alert(String(err));
      }
    }
  };

  return (
    <div className="ms-root" style={{ display: "flex", flexDirection: "column", height: "100%", padding: 20, overflowY: "auto" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 20 }}>
        <Server size={24} style={{ color: "var(--accent)" }} />
        <h2 style={{ margin: 0 }}>Sahte Sunucu (Mock Server & Localhost Spoofer)</h2>
      </div>
      
      <p style={{ color: "var(--muted)", marginBottom: 30, maxWidth: 800 }}>
        Bu araç, belirttiğiniz alan adını (Domain) bilgisayarınıza (127.0.0.1) yönlendirir ve o adrese gelen tüm isteklere sizin belirlediğiniz sahte cevabı döner. Özellikle internet üzerinden lisans doğrulaması yapan programları bypass etmek için kullanılır.
      </p>

      <div style={{ background: "#1a2030", padding: 20, borderRadius: 8, display: "flex", flexDirection: "column", gap: 16, maxWidth: 800 }}>
        <div style={{ display: "flex", gap: 16 }}>
          <div style={{ flex: 2 }}>
            <label style={{ display: "block", marginBottom: 6, fontSize: 13, color: "var(--muted)" }}>Yönlendirilecek Alan Adı (Spoof Domain)</label>
            <input 
              className="ms-input" 
              value={domain} 
              onChange={e => setDomain(e.target.value)} 
              disabled={active}
              style={{ width: "100%", padding: 8, background: "#0a0d14", border: "1px solid #2a3344", color: "#fff", borderRadius: 4 }} 
            />
          </div>
          <div style={{ flex: 1 }}>
            <label style={{ display: "block", marginBottom: 6, fontSize: 13, color: "var(--muted)" }}>Port (Örn: 80 HTTP)</label>
            <input 
              type="number"
              className="ms-input" 
              value={port} 
              onChange={e => setPort(Number(e.target.value))} 
              disabled={active}
              style={{ width: "100%", padding: 8, background: "#0a0d14", border: "1px solid #2a3344", color: "#fff", borderRadius: 4 }} 
            />
          </div>
        </div>

        <div>
          <label style={{ display: "block", marginBottom: 6, fontSize: 13, color: "var(--muted)" }}>Sahte Yanıt (Fake Response Payload)</label>
          <textarea 
            value={response} 
            onChange={e => setResponse(e.target.value)} 
            disabled={active}
            style={{ width: "100%", height: 120, padding: 8, background: "#0a0d14", border: "1px solid #2a3344", color: "#4db8ff", fontFamily: "var(--mono)", borderRadius: 4, resize: "vertical" }} 
          />
        </div>

        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginTop: 10 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, color: active ? "#4dff88" : "var(--muted)" }}>
            {active ? <ShieldCheck size={18} /> : <AlertTriangle size={18} />}
            <span style={{ fontSize: 13 }}>{active ? "/etc/hosts yönlendirildi, sunucu aktif." : "Sudo (Root) izni gereklidir."}</span>
          </div>
          <button 
            onClick={toggle} 
            style={{ 
              display: "flex", alignItems: "center", gap: 8, padding: "8px 16px", borderRadius: 6, border: "none", cursor: "pointer",
              background: active ? "rgba(255, 60, 60, 0.2)" : "var(--accent)", 
              color: active ? "#ff4d4d" : "#000",
              fontWeight: "bold"
            }}
          >
            {active ? <Square size={16} /> : <Play size={16} />}
            {active ? "Sunucuyu Durdur & Normale Dön" : "Yönlendirmeyi Başlat"}
          </button>
        </div>
      </div>

      {active && (
        <div style={{ marginTop: 30 }}>
          <h3 style={{ borderBottom: "1px solid #2a3344", paddingBottom: 10, marginBottom: 15 }}>Gelen İstekler (Intercepted Requests)</h3>
          {requests.length === 0 ? (
            <div style={{ padding: 40, textAlign: "center", color: "var(--muted)", background: "#1a2030", borderRadius: 8 }}>
              Henüz kimse {domain} adresine bağlanmadı. Bekleniyor...
            </div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              {requests.map((r, i) => (
                <div key={i} style={{ background: "#1a2030", padding: 12, borderRadius: 6, borderLeft: "3px solid #4db8ff" }}>
                  <div style={{ display: "flex", gap: 15, marginBottom: 5, fontSize: 13 }}>
                    <span style={{ color: "#4dff88", fontWeight: "bold" }}>{r.method}</span>
                    <span style={{ color: "#fff" }}>{r.path}</span>
                    <span style={{ color: "var(--muted)", marginLeft: "auto" }}>Kaynak: {r.ip}</span>
                  </div>
                  {r.body_preview && (
                    <div style={{ fontSize: 12, color: "var(--muted)", fontFamily: "var(--mono)", background: "#0a0d14", padding: 6, borderRadius: 4 }}>
                      {r.body_preview}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
