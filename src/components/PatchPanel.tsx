import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RotateCcw, Wand2, ShieldAlert, ShieldCheck } from "lucide-react";
import type { SoSymbol, PatchTemplate, PatchPreview, PrologueCheck, ApiError } from "../types";
import { PATCH_ASM, PATCH_LABEL } from "../patchInfo";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
type TemplateKind = PatchTemplate["kind"];

/** Boşlukla ayrılmış hex dizesini bayt kutucuklarına böler. */
function HexBytes({ hex, accent }: { hex: string; accent?: boolean }) {
  const bytes = hex.trim() ? hex.trim().split(/\s+/) : [];
  if (bytes.length === 0) return <span className="muted">—</span>;
  return (
    <span className="hex-bytes">
      {bytes.map((b, i) => (
        <span key={i} className={`hex-byte ${accent ? "hb-new" : ""}`}>
          {b}
        </span>
      ))}
    </span>
  );
}

export function PatchPanel({
  soPath,
  symbol,
  onChanged,
}: {
  soPath: string;
  symbol: SoSymbol;
  onChanged?: () => void;
}) {
  const [kind, setKind] = useState<TemplateKind>("return_true");
  const [value, setValue] = useState(1);
  const [preview, setPreview] = useState<PatchPreview | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [prologue, setPrologue] = useState<PrologueCheck | null>(null);

  const offset = symbol.file_offset;

  useEffect(() => {
    setResult(null);
    setError(null);
    setPreview(null);
    if (offset == null) return;
    const template: PatchTemplate = kind === "return_value" ? { kind, value } : { kind };
    invoke<PatchPreview>("patch_preview", { path: soPath, offset, template })
      .then(setPreview)
      .catch((e) => setError(errMsg(e)));
  }, [soPath, symbol, offset, kind, value]);

  // Güvenlik: seçili ofset gerçek bir fonksiyon başlangıcı mı?
  useEffect(() => {
    setPrologue(null);
    if (offset == null) return;
    invoke<PrologueCheck>("check_prologue", { path: soPath, offset, arch: "arm64" })
      .then(setPrologue)
      .catch(() => {});
  }, [soPath, offset]);

  if (offset == null) {
    return (
      <div className="patch-panel">
        <p className="muted">Bu sembolün dosya ofseti yok — yamalanamaz.</p>
      </div>
    );
  }

  const apply = async () => {
    const ok = window.confirm(
      `${symbol.name} (@0x${offset.toString(16)}) yamalanacak.\n` +
        `Orijinal dosya .bak olarak yedeklenecek. Devam edilsin mi?`
    );
    if (!ok) return;
    const template: PatchTemplate = kind === "return_value" ? { kind, value } : { kind };
    try {
      const backup = await invoke<string>("apply_so_patch", { path: soPath, offset, template });
      setResult(`Yama uygulandı. Yedek: ${backup}`);
      onChanged?.();
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const revert = async () => {
    setError(null);
    try {
      const msg = await invoke<string>("revert_patch_at", { path: soPath, offset });
      setResult(msg);
      onChanged?.();
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const newAsm = kind === "return_value" ? `MOV W0, #${value} ; RET` : PATCH_ASM[kind];

  return (
    <div className="patch-panel">
      <h3>
        Yama: <span className="mono">{symbol.name}</span>{" "}
        <span className="muted">@0x{offset.toString(16)}</span>
        {symbol.patched && (
          <span className="badge-modded" style={{ marginLeft: 8 }}>
            Kalıp: {PATCH_LABEL[symbol.patched] ?? symbol.patched}
          </span>
        )}
      </h3>

      {prologue && prologue.status === "not_prologue" && (
        <div className="prologue-warn">
          <ShieldAlert size={15} /> {prologue.warning}
        </div>
      )}
      {prologue && prologue.status === "prologue" && (
        <div className="prologue-ok">
          <ShieldCheck size={14} /> Fonksiyon başlangıcı doğrulandı: {prologue.detail}
        </div>
      )}

      <div className="patch-controls">
        <select value={kind} onChange={(e) => setKind(e.target.value as TemplateKind)}>
          <option value="return_true">return true (MOV W0,#1; RET)</option>
          <option value="return_false">return false (MOV W0,#0; RET)</option>
          <option value="return_value">return N (MOV W0,#N; RET)</option>
          <option value="return_max_int">return MAX_INT (0x7FFFFFFF)</option>
          <option value="nop">NOP</option>
          <option value="ret">RET</option>
        </select>
        {kind === "return_value" && (
          <input
            type="number"
            min={0}
            max={65535}
            value={value}
            onChange={(e) => setValue(Math.max(0, Math.min(65535, Number(e.target.value) || 0)))}
          />
        )}
      </div>

      {preview && (
        <div className="patch-diff">
          <div className="pd-col pd-before">
            <span className="field-label">ÖNCESİ (mevcut)</span>
            <HexBytes hex={preview.current} />
            <span className="pd-asm muted">
              {symbol.patched ? PATCH_ASM[symbol.patched] ?? symbol.patched : "orijinal kod"}
            </span>
          </div>
          <div className="pd-arrow">→</div>
          <div className="pd-col pd-after">
            <span className="field-label">SONRASI (yeni)</span>
            <HexBytes hex={preview.replacement} accent />
            <span className="pd-asm accent-text">{newAsm}</span>
          </div>
        </div>
      )}

      <div className="patch-actions">
        <button className="patch-apply" onClick={apply} disabled={!preview}>
          <Wand2 size={15} /> Yamayı uygula (.bak yedekli)
        </button>
        {symbol.patched && (
          <button className="patch-revert" onClick={revert} title="orijinal baytlara döndür">
            <RotateCcw size={15} /> Revert
          </button>
        )}
      </div>
      {result && <p className="patch-ok">{result}</p>}
      {error && <p className="error">Hata: {error}</p>}
    </div>
  );
}
