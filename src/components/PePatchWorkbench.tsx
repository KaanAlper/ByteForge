import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeftRight, Wand2, RotateCcw, Layers, ShieldAlert, ShieldCheck } from "lucide-react";
import type {
  PeSection,
  PeExport,
  PatchTemplate,
  PatchPreview,
  PrologueCheck,
  ApiError,
} from "../types";
import { PATCH_ASM, PATCH_LABEL } from "../patchInfo";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const hx = (n: number) => "0x" + n.toString(16).toUpperCase();

function parseAddr(s: string): number | null {
  const c = s.trim().toLowerCase().replace(/^0x/, "");
  return /^[0-9a-f]+$/.test(c) ? parseInt(c, 16) : null;
}

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

type Kind = PatchTemplate["kind"];

export function PePatchWorkbench({ pePath }: { pePath: string }) {
  const [sections, setSections] = useState<PeSection[]>([]);
  const [exports, setExports] = useState<PeExport[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [q, setQ] = useState("");
  const [rva, setRva] = useState("");
  const [off, setOff] = useState("");
  const [rvaOut, setRvaOut] = useState<string | null>(null);
  const [offOut, setOffOut] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Seçili yama hedefi
  const [selName, setSelName] = useState<string | null>(null);
  const [selOffset, setSelOffset] = useState<number | null>(null);
  const [selPatched, setSelPatched] = useState<string | null>(null);
  const [kind, setKind] = useState<Kind>("return_true");
  const [preview, setPreview] = useState<PatchPreview | null>(null);
  const [prologue, setPrologue] = useState<PrologueCheck | null>(null);
  const [result, setResult] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [secs, exps] = await Promise.all([
        invoke<PeSection[]>("pe_sections", { path: pePath }),
        invoke<PeExport[]>("pe_exports", { path: pePath }),
      ]);
      setSections(secs);
      setExports(exps);
      setLoaded(true);
    } catch (e) {
      setError(errMsg(e));
    }
  }, [pePath]);

  useEffect(() => {
    if (!preview && selOffset == null) return;
    if (selOffset == null) return;
    const template: PatchTemplate = { kind } as PatchTemplate;
    invoke<PatchPreview>("pe_patch_preview", { path: pePath, offset: selOffset, template })
      .then(setPreview)
      .catch((e) => setError(errMsg(e)));
  }, [pePath, selOffset, kind, preview]);

  useEffect(() => {
    setPrologue(null);
    if (selOffset == null) return;
    invoke<PrologueCheck>("check_prologue", { path: pePath, offset: selOffset, arch: "x86" })
      .then(setPrologue)
      .catch(() => {});
  }, [pePath, selOffset]);

  const convertRva = async () => {
    setError(null);
    setOffOut(null);
    const n = parseAddr(rva);
    if (n === null) return setError("geçersiz RVA (hex)");
    try {
      const o = await invoke<number | null>("pe_rva_to_offset", { path: pePath, rva: n });
      setOffOut(o === null ? "dosyada karşılığı yok" : hx(o));
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const convertOff = async () => {
    setError(null);
    setRvaOut(null);
    const n = parseAddr(off);
    if (n === null) return setError("geçersiz ofset (hex)");
    try {
      const r = await invoke<number | null>("pe_offset_to_rva", { path: pePath, offset: n });
      setRvaOut(r === null ? "section dışı" : hx(r));
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const selectExport = (e: PeExport) => {
    if (e.file_offset == null) {
      setError(`${e.name}: dosya ofseti yok — yamalanamaz`);
      return;
    }
    setSelName(e.name);
    setSelOffset(e.file_offset);
    setSelPatched(e.patched);
    setResult(null);
    setError(null);
    setPreview(null);
  };

  const selectManual = () => {
    const n = parseAddr(off);
    if (n === null) return setError("Ofset → RVA alanına geçerli hex ofset girin");
    setSelName("(manuel ofset)");
    setSelOffset(n);
    setSelPatched(null);
    setResult(null);
    setPreview(null);
  };

  const apply = async () => {
    if (selOffset == null) return;
    const ok = window.confirm(
      `${selName} (@${hx(selOffset)}) yamalanacak.\n.bak yedeği alınacak. Devam?`
    );
    if (!ok) return;
    try {
      const backup = await invoke<string>("apply_pe_patch", {
        path: pePath,
        offset: selOffset,
        template: { kind } as PatchTemplate,
      });
      setResult(`Yama uygulandı. Yedek: ${backup}`);
      load();
      setSelPatched(kind);
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const revert = async () => {
    if (selOffset == null) return;
    try {
      const msg = await invoke<string>("revert_patch_at", { path: pePath, offset: selOffset });
      setResult(msg);
      setSelPatched(null);
      load();
    } catch (e) {
      setError(errMsg(e));
    }
  };

  if (!loaded) {
    return (
      <div className="deploy-actions" style={{ marginTop: 14 }}>
        <button className="primary-btn" onClick={load}>
          <Wand2 size={15} /> Yamalama tezgahını aç (section + export + x86 yama)
        </button>
        {error && <p className="error">Hata: {error}</p>}
      </div>
    );
  }

  const ql = q.toLowerCase();
  const filtered = exports.filter((e) => e.name.toLowerCase().includes(ql));
  const patchedCount = exports.filter((e) => e.patched).length;
  const newAsm = PATCH_ASM[kind];

  return (
    <div className="pe-workbench">
      {/* RVA ↔ Ofset */}
      <div className="addr-body" style={{ padding: 0, marginTop: 14 }}>
        <div className="manifest-head">
          <span className="field-label">
            <ArrowLeftRight size={13} style={{ verticalAlign: "-0.15em" }} /> RVA ↔ Ofset (PE)
          </span>
        </div>
        <div className="addr-grid" style={{ marginTop: 8 }}>
          <div className="addr-cell">
            <span className="field-label">RVA → Ofset</span>
            <div className="inline-field">
              <input
                placeholder="ör. 2100"
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
                placeholder="ör. 300"
                value={off}
                onChange={(e) => setOff(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && convertOff()}
              />
              <button onClick={convertOff}>çevir</button>
              <button onClick={selectManual} title="bu ofseti yamala">yamala</button>
            </div>
            {rvaOut && <div className="addr-out mono">→ {rvaOut}</div>}
          </div>
        </div>
      </div>

      {/* Section haritası */}
      <div className="manifest-head" style={{ marginTop: 16 }}>
        <span className="field-label">
          <Layers size={13} style={{ verticalAlign: "-0.15em" }} /> Section'lar
        </span>
      </div>
      <div className="seg-table mono">
        <div className="seg-row seg-head">
          <span>ad · RVA</span>
          <span>dosya ofseti</span>
          <span>ham boyut</span>
          <span>izin</span>
        </div>
        {sections.map((s, i) => (
          <div key={i} className={`seg-row ${s.flags.endsWith("X") ? "seg-exec" : ""}`}>
            <span>{s.name} · {hx(s.virtual_address)}</span>
            <span>{hx(s.raw_pointer)}</span>
            <span>{hx(s.raw_size)}</span>
            <span className="seg-flags">{s.flags}</span>
          </div>
        ))}
      </div>

      {/* Export tablosu */}
      <div className="manifest-head" style={{ marginTop: 16 }}>
        <span className="field-label">
          Export'lar ({exports.length})
          {patchedCount > 0 && <span className="badge-modded-inline"> · {patchedCount} kalıp eşleşmesi</span>}
        </span>
      </div>
      {exports.length === 0 ? (
        <p className="muted">Bu PE adlı export sağlamıyor (DLL değil ya da tablo yok). RVA→Ofset çevirip manuel yamalayın.</p>
      ) : (
        <>
          <input
            className="il2cpp-search"
            placeholder="export ara…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            style={{ marginBottom: 8 }}
          />
          <div className="pe-exports">
            {filtered.slice(0, 400).map((e, i) => (
              <div
                key={i}
                className={`pe-export ${selName === e.name ? "sel" : ""}`}
                onClick={() => selectExport(e)}
              >
                {e.patched && (
                  <span className="badge-modded" title={`İlk baytlar "${PATCH_LABEL[e.patched]}" kalıbında — yamalı VEYA zaten öyle döndürüyor.`}>
                    = {PATCH_LABEL[e.patched]}
                  </span>
                )}
                <span className="mono pe-exp-name">{e.name}</span>
                <span className="mono muted">{hx(e.rva)}</span>
              </div>
            ))}
          </div>
        </>
      )}

      {error && <p className="error">Hata: {error}</p>}

      {/* Yama paneli */}
      {selOffset != null && (
        <div className="patch-panel" style={{ marginTop: 14 }}>
          <h3>
            Yama: <span className="mono">{selName}</span>{" "}
            <span className="muted">@{hx(selOffset)}</span>
            {selPatched && (
              <span className="badge-modded" style={{ marginLeft: 8 }}>
                Kalıp: {PATCH_LABEL[selPatched] ?? selPatched}
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
            <select value={kind} onChange={(e) => setKind(e.target.value as Kind)}>
              <option value="return_true">return true (mov eax,1; ret)</option>
              <option value="return_false">return false (xor eax,eax; ret)</option>
              <option value="return_max_int">return MAX_INT (0x7FFFFFFF)</option>
              <option value="nop">NOP (0x90)</option>
              <option value="ret">RET (0xC3)</option>
            </select>
          </div>
          {preview && (
            <div className="patch-diff">
              <div className="pd-col pd-before">
                <span className="field-label">ÖNCESİ</span>
                <HexBytes hex={preview.current} />
                <span className="pd-asm muted">
                  {selPatched ? PATCH_ASM[selPatched] ?? selPatched : "orijinal kod"}
                </span>
              </div>
              <div className="pd-arrow">→</div>
              <div className="pd-col pd-after">
                <span className="field-label">SONRASI</span>
                <HexBytes hex={preview.replacement} accent />
                <span className="pd-asm accent-text">{newAsm}</span>
              </div>
            </div>
          )}
          <div className="patch-actions">
            <button className="patch-apply" onClick={apply} disabled={!preview}>
              <Wand2 size={15} /> Yamayı uygula (.bak yedekli)
            </button>
            {selPatched && (
              <button className="patch-revert" onClick={revert}>
                <RotateCcw size={15} /> Revert
              </button>
            )}
          </div>
          {result && <p className="patch-ok">{result}</p>}
        </div>
      )}
    </div>
  );
}
