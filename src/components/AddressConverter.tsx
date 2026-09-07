import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeftRight, Layers } from "lucide-react";
import type { ElfSegment, ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const hx = (n: number) => "0x" + n.toString(16).toUpperCase();

/** "0x1FB7100" / "1FB7100" → sayı (hex olarak yorumlanır); geçersizse null. */
function parseAddr(s: string): number | null {
  const clean = s.trim().toLowerCase().replace(/^0x/, "");
  if (!clean || !/^[0-9a-f]+$/.test(clean)) return null;
  const n = parseInt(clean, 16);
  return Number.isFinite(n) && n >= 0 ? n : null;
}

/**
 * RVA ↔ dosya ofseti dönüştürücü — bir dumper/IDA'nın gösterdiği RVA'yı
 * libil2cpp.so içinde yamalanacak dosya ofsetine (ve tersine) çevirir.
 * ELF program header'larından hesaplanır: metadata sürümünden/obfuscation'dan
 * bağımsız, kesin.
 */
export function AddressConverter({ soPath }: { soPath: string }) {
  const [segments, setSegments] = useState<ElfSegment[]>([]);
  const [rva, setRva] = useState("");
  const [off, setOff] = useState("");
  const [rvaOut, setRvaOut] = useState<string | null>(null);
  const [offOut, setOffOut] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open || segments.length) return;
    invoke<ElfSegment[]>("so_segments", { path: soPath })
      .then(setSegments)
      .catch((e) => setError(errMsg(e)));
  }, [open, soPath, segments.length]);

  const convertRva = async () => {
    setError(null);
    setOffOut(null);
    const n = parseAddr(rva);
    if (n === null) {
      setError("geçersiz RVA (hex bekleniyor, ör. 1FB7100)");
      return;
    }
    try {
      const o = await invoke<number | null>("rva_to_offset", { path: soPath, rva: n });
      setOffOut(o === null ? "segmente düşmüyor (ör. .bss)" : hx(o));
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const convertOff = async () => {
    setError(null);
    setRvaOut(null);
    const n = parseAddr(off);
    if (n === null) {
      setError("geçersiz ofset (hex bekleniyor)");
      return;
    }
    try {
      const r = await invoke<number | null>("offset_to_rva", { path: soPath, offset: n });
      setRvaOut(r === null ? "segmente düşmüyor" : hx(r));
    } catch (e) {
      setError(errMsg(e));
    }
  };

  return (
    <div className="addr-conv">
      <button className="addr-toggle" onClick={() => setOpen((s) => !s)}>
        <ArrowLeftRight size={15} />
        {open ? "Adres dönüştürücüyü gizle" : "RVA ↔ Ofset dönüştürücü (IL2CPP)"}
      </button>

      {open && (
        <div className="addr-body">
          <p className="muted" style={{ margin: "0 0 12px", fontSize: 13 }}>
            Dumper/IDA bir metodu <b>RVA</b> olarak gösterir; buraya girip dosya
            ofsetini alın, Hex sekmesinde o ofsete opcode yazın. ELF segmentlerinden
            hesaplanır — metadata sürümünden bağımsız.
          </p>

          <div className="addr-grid">
            <div className="addr-cell">
              <span className="field-label">RVA → Ofset</span>
              <div className="inline-field">
                <input
                  placeholder="ör. 1FB7100"
                  value={rva}
                  onChange={(e) => setRva(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && convertRva()}
                />
                <button onClick={convertRva}>çevir</button>
              </div>
              {offOut && <div className="addr-out mono">→ {offOut}</div>}
            </div>

            <div className="addr-cell">
              <span className="field-label">Ofset → RVA</span>
              <div className="inline-field">
                <input
                  placeholder="ör. 1FB3100"
                  value={off}
                  onChange={(e) => setOff(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && convertOff()}
                />
                <button onClick={convertOff}>çevir</button>
              </div>
              {rvaOut && <div className="addr-out mono">→ {rvaOut}</div>}
            </div>
          </div>

          {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}

          <div className="manifest-head" style={{ marginTop: 14 }}>
            <span className="field-label">
              <Layers size={13} style={{ verticalAlign: "-0.15em" }} /> LOAD segmentleri
            </span>
          </div>
          <div className="seg-table mono">
            <div className="seg-row seg-head">
              <span>vaddr (RVA)</span>
              <span>dosya ofseti</span>
              <span>boyut</span>
              <span>izin</span>
            </div>
            {segments.map((s) => (
              <div key={s.index} className={`seg-row ${s.flags.endsWith("X") ? "seg-exec" : ""}`}>
                <span>{hx(s.vaddr)}</span>
                <span>{hx(s.file_offset)}</span>
                <span>{hx(s.file_size)}</span>
                <span className="seg-flags">{s.flags}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
