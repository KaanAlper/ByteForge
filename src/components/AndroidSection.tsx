import { useState, useEffect } from "react";
import { ScanSearch, Crosshair, Gamepad2, Rocket } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { ProfileCard } from "./ProfileCard";
import { Il2CppPanel } from "./Il2CppPanel";
import { ModMenuStudio } from "./ModMenuStudio";
import { DeployPanel } from "./DeployPanel";
import { SplitPanel } from "./SplitPanel";
import type { AppProfile } from "../types";

type StepId = "overview" | "patch" | "menu" | "deploy";

const STEPS: { id: StepId; label: string; hint: string; icon: LucideIcon }[] = [
  { id: "overview", label: "Genel Bakış", hint: "Bu APK ne?", icon: ScanSearch },
  { id: "patch", label: "Hedefler & Yama", hint: "Fonksiyon bul, yamala", icon: Crosshair },
  { id: "menu", label: "Mod Menü", hint: "Yüzen menü enjekte et", icon: Gamepad2 },
  { id: "deploy", label: "Dağıt & Kur", hint: "İmzala, telefona kur", icon: Rocket },
];

/**
 * Android modlama akışı — tek bölüm, sıralı adımlar. Bir APK'yı anlamaktan
 * (Genel Bakış) yamaya (Hedefler & Yama), oradan mod menüsüne ve telefona
 * kurmaya (Dağıt) mantıksal ilerleme. Eski Profil/Native/Mod Menü/Dağıt
 * sekmelerinin yerini alır.
 */
export function AndroidSection({
  profile,
  apkPath,
}: {
  profile: AppProfile;
  apkPath: string | null;
}) {
  const [step, setStep] = useState<StepId>("overview");
  const isIl2cpp = profile.runtime === "unity_il2cpp";

  // Keep-alive: ziyaret edilen adımlar DOM'da kalır (gizlenir), unmount olmaz —
  // böylece IL2CPP taraması, Mod Menü özellikleri, yapılan işlemler kaybolmaz.
  const [mounted, setMounted] = useState<Set<StepId>>(() => new Set<StepId>(["overview"]));
  useEffect(() => {
    setMounted((m) => (m.has(step) ? m : new Set(m).add(step)));
  }, [step]);

  return (
    <div className="android-section">
      <nav className="android-steps" aria-label="Android modlama adımları">
        {STEPS.map((s, i) => (
          <button
            key={s.id}
            className={`android-step ${step === s.id ? "active" : ""}`}
            onClick={() => setStep(s.id)}
          >
            <span className="android-step-num">{i + 1}</span>
            <span className="android-step-text">
              <span className="android-step-label">{s.label}</span>
              <span className="android-step-hint">{s.hint}</span>
            </span>
          </button>
        ))}
      </nav>

      <div className="android-step-body">
        {mounted.has("overview") && (
          <div hidden={step !== "overview"}>
            <ProfileCard profile={profile} apkPath={apkPath ?? undefined} />
          </div>
        )}

        {mounted.has("patch") && (
          <div hidden={step !== "patch"}>
            {isIl2cpp && apkPath ? (
              <Il2CppPanel apkPath={apkPath} />
            ) : (
              <div className="panel-empty">
                <Crosshair size={30} />
                <p>
                  Bu motor ({profile.runtime}) için native yama akışı yok. Java/Kotlin kodunu{" "}
                  <b>Smali</b> veya <b>Java</b> sekmesinde decompile edip yamalayın.
                </p>
              </div>
            )}
          </div>
        )}

        {mounted.has("menu") && (
          <div hidden={step !== "menu"}>
            {apkPath ? (
              <ModMenuStudio key={apkPath} apkPath={apkPath} runtime={profile.runtime} />
            ) : (
              <div className="panel-empty">
                <Gamepad2 size={30} />
                <p>Önce bir APK yükleyin — mod menüsü buraya enjekte edilir.</p>
              </div>
            )}
          </div>
        )}

        {mounted.has("deploy") && (
          <div hidden={step !== "deploy"}>
            {apkPath ? (
              <>
                <DeployPanel apkPath={apkPath} packageName={profile.manifest?.package ?? null} />
                {/\.(xapk|apks)$/i.test(apkPath) && (
                  <div style={{ marginTop: 16 }}>
                    <SplitPanel apkPath={apkPath} />
                  </div>
                )}
              </>
            ) : (
              <div className="panel-empty">
                <Rocket size={30} />
                <p>Önce bir APK yükleyin.</p>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
