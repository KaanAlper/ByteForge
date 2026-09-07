import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Calculator, X } from "lucide-react";

interface DeobfCandidate {
  method: string;
  key: number | null;
  text: string;
  score: number;
}

/** Basit ARM64 mini-assembler: RET, NOP, MOV W0/X0, #imm (imm 16-bit). */
function assemble(input: string): string | null {
  const s = input.trim().toUpperCase().replace(/,/g, " ").replace(/\s+/g, " ");
  if (s === "RET") return "C0 03 5F D6";
  if (s === "NOP") return "1F 20 03 D5";
  // MOV W0 #N  → MOVZ W0, #imm (0x52800000 | imm<<5)
  const m = s.match(/^MOV (W0|X0) #(0X[0-9A-F]+|\d+)$/);
  if (m) {
    const imm = m[2].startsWith("0X") ? parseInt(m[2], 16) : parseInt(m[2], 10);
    if (imm < 0 || imm > 0xffff) return null; // tek MOVZ 16-bit
    const base = m[1] === "X0" ? 0xd2800000 : 0x52800000;
    const op = (base | (imm << 5)) >>> 0;
    const b = [op & 0xff, (op >> 8) & 0xff, (op >> 16) & 0xff, (op >> 24) & 0xff];
    return b.map((x) => x.toString(16).padStart(2, "0").toUpperCase()).join(" ");
  }
  return null;
}

export function Scratchpad() {
  const [open, setOpen] = useState(false);
  const [num, setNum] = useState("");
  const [asm, setAsm] = useState("");
  const [enc, setEnc] = useState("");
  const [cands, setCands] = useState<DeobfCandidate[] | null>(null);

  const decode = async () => {
    if (!enc.trim()) {
      setCands(null);
      return;
    }
    try {
      setCands(await invoke<DeobfCandidate[]>("deobfuscate_string", { input: enc }));
    } catch {
      setCands([]);
    }
  };

  const n = num.trim().startsWith("0x")
    ? parseInt(num.trim(), 16)
    : parseInt(num.trim(), 10);
  const validNum = num.trim() !== "" && !Number.isNaN(n);
  const asmHex = asm.trim() ? assemble(asm) : null;

  if (!open) {
    return (
      <button className="scratch-fab" onClick={() => setOpen(true)} title="Hex/ASM çevirici">
        <Calculator size={20} />
      </button>
    );
  }

  return (
    <div className="scratch-drawer">
      <div className="scratch-head">
        <span>Çevirici</span>
        <button onClick={() => setOpen(false)} title="kapat"><X size={16} /></button>
      </div>

      <label className="scratch-label">Sayı → hex / ARM64</label>
      <input
        placeholder="ör. 999999 veya 0x1F"
        value={num}
        onChange={(e) => setNum(e.target.value)}
      />
      {validNum && (
        <div className="scratch-out mono">
          <div>dec: {n.toLocaleString()}</div>
          <div>hex: 0x{(n >>> 0).toString(16).toUpperCase()}</div>
          {n >= 0 && n <= 0xffff && <div>MOV W0, #{n}: {assemble(`MOV W0, #${n}`)}</div>}
        </div>
      )}

      <label className="scratch-label">ARM64 → hex</label>
      <input
        placeholder="RET · NOP · MOV W0, #1"
        value={asm}
        onChange={(e) => setAsm(e.target.value)}
      />
      {asm.trim() && (
        <div className="scratch-out mono">
          {asmHex ? asmHex : <span className="scratch-err">tanınmadı</span>}
        </div>
      )}

      <p className="scratch-hint">RET, NOP, MOV W0/X0, #imm (16-bit) desteklenir.</p>

      <label className="scratch-label">String çöz (Base64 · hex · XOR · ROT13)</label>
      <input
        placeholder="gizlenmiş dizeyi yapıştırın"
        value={enc}
        onChange={(e) => setEnc(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && decode()}
      />
      <button className="scratch-decode" onClick={decode} disabled={!enc.trim()}>
        çöz
      </button>
      {cands && (
        <div className="scratch-cands">
          {cands.length === 0 ? (
            <span className="scratch-err">okunabilir çözüm bulunamadı</span>
          ) : (
            cands.map((c, i) => (
              <div key={i} className="scratch-cand">
                <span className="scratch-cand-m">
                  {c.method}
                  {c.key !== null ? ` 0x${c.key.toString(16).toUpperCase()}` : ""}
                </span>
                <code className="mono">{c.text}</code>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
