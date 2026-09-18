import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Play, Square, Trash2, AlertTriangle, Search } from "lucide-react";

interface PacketInfo {
  id: number;
  protocol: string;
  src: string;
  dst: string;
  length: number;
  payload_preview: string;
}

export function NetworkSniffer() {
  const [isRoot, setIsRoot] = useState<boolean | null>(null);
  const [active, setActive] = useState(false);
  const [packets, setPackets] = useState<PacketInfo[]>([]);
  const [filter, setFilter] = useState("");
  const [autoScroll, setAutoScroll] = useState(true);
  
  const endRef = useRef<HTMLTableRowElement>(null);

  useEffect(() => {
    invoke<boolean>("is_root").then(setIsRoot);
  }, []);

  useEffect(() => {
    let unlistenPkt: () => void;
    let unlistenErr: () => void;
    
    listen<PacketInfo>("sniffer-packet", (e) => {
      setPackets((prev) => {
        const next = [...prev, e.payload];
        if (next.length > 500) next.shift(); // keep last 500
        return next;
      });
    }).then(u => unlistenPkt = u);
    
    listen<string>("sniffer-error", (e) => {
      alert("Sniffer Hatası: " + e.payload);
      setActive(false);
    }).then(u => unlistenErr = u);
    
    return () => {
      if (unlistenPkt) unlistenPkt();
      if (unlistenErr) unlistenErr();
      invoke("stop_sniffer");
    };
  }, []);

  useEffect(() => {
    if (autoScroll && endRef.current) {
      endRef.current.scrollIntoView();
    }
  }, [packets, autoScroll]);

  const toggle = async () => {
    if (active) {
      await invoke("stop_sniffer");
      setActive(false);
    } else {
      try {
        await invoke("start_sniffer");
        setActive(true);
      } catch (err) {
        alert(String(err));
      }
    }
  };

  const clear = async () => {
    await invoke("clear_sniffer");
    setPackets([]);
  };
  
  const filtered = packets.filter(p => 
    !filter || 
    p.src.includes(filter) || 
    p.dst.includes(filter) || 
    p.protocol.toLowerCase().includes(filter.toLowerCase()) ||
    p.payload_preview.toLowerCase().includes(filter.toLowerCase())
  );

  if (isRoot === false) {
    return (
      <div className="ms-root" style={{ padding: 40, textAlign: "center" }}>
        <AlertTriangle size={48} style={{ color: "#ff7a3c", marginBottom: 20 }} />
        <h2>Root İzni Gerekli</h2>
        <p style={{ color: "var(--muted)", maxWidth: 400, margin: "0 auto 20px" }}>
          Linux çekirdek yapısı gereği, ağ paketlerini canlı olarak dinleyebilmek için uygulamanın en yüksek yetkilerle çalışması gerekir.
        </p>
        <div style={{ background: "#1a2030", padding: "10px 20px", borderRadius: 6, display: "inline-block", fontFamily: "var(--mono)" }}>
          sudo byteforge
        </div>
      </div>
    );
  }

  return (
    <div className="ms-root" style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div className="ms-topbar" style={{ flexShrink: 0 }}>
        <div className="ms-topbar-left">
          <button className={`ms-btn-secondary ${active ? "active" : ""}`} onClick={toggle} style={{ background: active ? "rgba(255, 60, 60, 0.2)" : undefined, color: active ? "#ff4d4d" : undefined }}>
            {active ? <Square size={14} /> : <Play size={14} />}
            {active ? "Durdur" : "Başlat"}
          </button>
          <button className="ms-btn-secondary" onClick={clear}>
            <Trash2 size={14} /> Temizle
          </button>
          <div className="ms-search">
            <Search size={14} />
            <input 
              placeholder="IP, Protokol veya İçerik ara..." 
              value={filter} 
              onChange={e => setFilter(e.target.value)} 
              style={{ width: 200 }}
            />
          </div>
        </div>
        <div className="ms-topbar-right">
          <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13, cursor: "pointer", color: "var(--text)" }}>
            <input type="checkbox" checked={autoScroll} onChange={e => setAutoScroll(e.target.checked)} />
            Oto-Kaydır
          </label>
        </div>
      </div>
      
      <div style={{ flex: 1, overflowY: "auto", background: "#0a0d14", borderTop: "1px solid var(--border)" }}>
        <table className="ms-ct-table" style={{ width: "100%" }}>
          <thead>
            <tr>
              <th style={{ width: 60 }}>No</th>
              <th style={{ width: 80 }}>Protokol</th>
              <th style={{ width: 130 }}>Kaynak IP</th>
              <th style={{ width: 130 }}>Hedef IP</th>
              <th style={{ width: 80 }}>Boyut</th>
              <th>İçerik (Payload)</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map(p => (
              <tr key={p.id} style={{ borderBottom: "1px solid #1a2030" }}>
                <td className="mono" style={{ color: "var(--muted)" }}>{p.id}</td>
                <td style={{ color: p.protocol === "TCP" ? "#4db8ff" : "#ff9e4d", fontWeight: "bold" }}>{p.protocol}</td>
                <td className="mono">{p.src}</td>
                <td className="mono">{p.dst}</td>
                <td className="mono">{p.length} B</td>
                <td className="mono" style={{ color: "#a8b2c8", wordBreak: "break-all" }}>{p.payload_preview}</td>
              </tr>
            ))}
            {filtered.length === 0 && (
              <tr>
                <td colSpan={6} style={{ textAlign: "center", padding: 40, color: "var(--muted)" }}>
                  {active ? "Paket dinleniyor..." : "Başlat butonuna basarak dinlemeyi başlatın."}
                </td>
              </tr>
            )}
            <tr ref={endRef} />
          </tbody>
        </table>
      </div>
    </div>
  );
}
