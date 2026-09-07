import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SoSymbol } from "../types";
import { SymbolTable } from "./SymbolTable";
import { PatchPanel } from "./PatchPanel";
import { HexEditor } from "./HexEditor";
import { AddressConverter } from "./AddressConverter";
import { AntiTamper } from "./AntiTamper";

export function NativePanel({
  soPath,
  symbols,
  initialQuery = "",
}: {
  soPath: string;
  symbols: SoSymbol[];
  initialQuery?: string;
}) {
  const [syms, setSyms] = useState<SoSymbol[]>(symbols);
  const [selected, setSelected] = useState<SoSymbol | null>(null);
  const [showHex, setShowHex] = useState(false);

  // Yama/revert sonrası sembolleri (MODLU rozetleriyle) yeniden yükle.
  const reload = useCallback(async () => {
    try {
      const fresh = await invoke<SoSymbol[]>("list_so_symbols", { path: soPath });
      setSyms(fresh);
      // Seçili sembolü ad+rva ile eşleştirip güncel patched durumuyla tut.
      setSelected((prev) =>
        prev ? fresh.find((s) => s.name === prev.name && s.rva === prev.rva) ?? null : null
      );
    } catch {
      /* yoksay */
    }
  }, [soPath]);

  return (
    <div className="native-panel">
      <AntiTamper command="antitamper_so" path={soPath} />
      <AddressConverter soPath={soPath} />
      <SymbolTable
        symbols={syms}
        onSelect={setSelected}
        selected={selected}
        initialQuery={initialQuery}
      />
      {selected && <PatchPanel soPath={soPath} symbol={selected} onChanged={reload} />}
      <div className="hex-toggle">
        <button onClick={() => setShowHex((s) => !s)}>
          {showHex ? "Hex editörü gizle" : "Hex editörü aç (ham bayt)"}
        </button>
      </div>
      {showHex && <HexEditor path={soPath} />}
    </div>
  );
}
