import { Gamepad2, Zap } from "lucide-react";
import type { RuntimeKind } from "../types";

/**
 * Smali/Java sekmelerinin üstünde, oyun motoru IL2CPP/Mono ise kullanıcıyı
 * doğru tezgaha yönlendiren bilgi kartı. Diğer motorlarda görünmez.
 */
export function EngineHint({
  runtime,
  onGoEngine,
}: {
  runtime: RuntimeKind | undefined;
  onGoEngine: () => void;
}) {
  if (runtime === "unity_il2cpp") {
    return (
      <div className="engine-hint">
        <div className="engine-hint-body">
          <Gamepad2 size={18} className="engine-hint-ic" />
          <div>
            <b>Bu bir Unity (IL2CPP) oyunudur.</b> Karakterler, altınlar, kilitler ve
            tüm oyun mantığı <code className="mono">libil2cpp.so</code> içindedir. Buradaki
            Smali/Java kodu yalnızca reklam/analitik ve Android kabuğudur.
          </div>
        </div>
        <button className="engine-hint-btn" onClick={onGoEngine}>
          <Zap size={15} /> IL2CPP Analizine Git
        </button>
      </div>
    );
  }
  if (runtime === "unity_mono") {
    return (
      <div className="engine-hint">
        <div className="engine-hint-body">
          <Gamepad2 size={18} className="engine-hint-ic" />
          <div>
            <b>Bu bir Unity (Mono) oyunudur.</b> Oyun mantığı{" "}
            <code className="mono">Assembly-CSharp.dll</code> içindedir (IL). Smali/Java
            yalnızca Android kabuğudur.
          </div>
        </div>
        <button className="engine-hint-btn" onClick={onGoEngine}>
          <Zap size={15} /> Profil / Mono Analizine Git
        </button>
      </div>
    );
  }
  if (runtime === "flutter") {
    return (
      <div className="engine-hint">
        <div className="engine-hint-body">
          <Gamepad2 size={18} className="engine-hint-ic" />
          <div>
            <b>Bu bir Flutter uygulamasıdır.</b> Mantık{" "}
            <code className="mono">libapp.so</code> (Dart AOT) içindedir; Smali/Java yalnızca
            kabuktur.
          </div>
        </div>
        <button className="engine-hint-btn" onClick={onGoEngine}>
          <Zap size={15} /> Native Analizine Git
        </button>
      </div>
    );
  }
  return null;
}
