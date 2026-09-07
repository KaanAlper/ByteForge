import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Copy, Check } from "lucide-react";
import type { ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

/** Backend PatchTemplate enum'unun serde temsili (#[serde(tag="kind", content="value")]). */
type Template =
  | { kind: "return_true" }
  | { kind: "return_false" }
  | { kind: "return_max_int" }
  | { kind: "nop" }
  | { kind: "ret" };

type Arch = "arm64" | "x86";

const ROWS: { t: Template; label: string; desc: string }[] = [
  { t: { kind: "return_true" }, label: "Return True", desc: "fonksiyon her zaman true/1 döner" },
  { t: { kind: "return_false" }, label: "Return False", desc: "her zaman false/0 döner" },
  { t: { kind: "return_max_int" }, label: "Return MAX_INT", desc: "0x7FFFFFFF döner (para/sayaç)" },
  { t: { kind: "nop" }, label: "NOP", desc: "komutu etkisiz kılar" },
  { t: { kind: "ret" }, label: "RET", desc: "hemen döner (gövdeyi atla)" },
];

/**
 * Belirtilen mimarideki hazır opcode şablonlarını backend'den çekip
 * kopyalanabilir bir referans tablosu olarak gösterir. Yama uygulamaz —
 * baytları Hex sekmesinde ilgili ofsete yazmak için kullanın.
 */
export function OpcodePalette({ arch }: { arch: Arch }) {
  const [hexes, setHexes] = useState<Record<string, string>>({});
  const [copied, setCopied] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const out: Record<string, string> = {};
        for (const r of ROWS) {
          out[r.label] = await invoke<string>("opcode_hex", { template: r.t, arch });
        }
        if (alive) setHexes(out);
      } catch (e) {
        if (alive) setError(errMsg(e));
      }
    })();
    return () => {
      alive = false;
    };
  }, [arch]);

  const copy = async (label: string, hex: string) => {
    try {
      await navigator.clipboard.writeText(hex);
      setCopied(label);
      setTimeout(() => setCopied((c) => (c === label ? null : c)), 1200);
    } catch {
      /* pano erişimi yoksa sessiz geç */
    }
  };

  const archLabel = arch === "x86" ? "x86 / x64" : "ARM64";

  return (
    <div className="opcode-palette">
      <div className="manifest-head">
        <span className="field-label">Opcode şablonları — {archLabel}</span>
        <span className="muted">kopyala → Hex sekmesinde ofsete yaz</span>
      </div>
      {error && <p className="error">Hata: {error}</p>}
      <div className="opcode-rows">
        {ROWS.map((r) => {
          const hex = hexes[r.label] ?? "…";
          return (
            <div key={r.label} className="opcode-row">
              <div className="opcode-meta">
                <span className="opcode-name">{r.label}</span>
                <span className="opcode-desc muted">{r.desc}</span>
              </div>
              <code className="opcode-hex mono">{hex}</code>
              <button
                className="opcode-copy"
                onClick={() => copy(r.label, hex)}
                disabled={hex === "…"}
                title="baytları kopyala"
              >
                {copied === r.label ? <Check size={14} /> : <Copy size={14} />}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
