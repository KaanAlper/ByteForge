import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { listen } from "@tauri-apps/api/event";
import type { AppProfile, ToolReport, ApiError, ArchiveDiff, SoSymbol } from "./types";
import { Disclaimer } from "./components/Disclaimer";
import { DiffView } from "./components/DiffView";
import { NativePanel } from "./components/NativePanel";
import { AndroidSection } from "./components/AndroidSection";
import { SmaliWorkbench } from "./components/SmaliWorkbench";
import { JavaWorkbench } from "./components/JavaWorkbench";
import { HexEditor } from "./components/HexEditor";
import { Scratchpad } from "./components/Scratchpad";
import { HistoryPanel } from "./components/HistoryPanel";
import { SplitPanel } from "./components/SplitPanel";
import { ConsolePanel } from "./components/ConsolePanel";
import { SettingsPanel } from "./components/SettingsPanel";
import { PePanel } from "./components/PePanel";
import { FridaPanel } from "./components/FridaPanel";
import { MemScanner } from "./components/MemScanner";
import {
  Smartphone,
  Binary,
  AppWindow,
  ScrollText,
  Coffee,
  Hash,
  GitCompare,
  Split,
  Terminal,
  History,
  Settings,
  FolderOpen,
  LoaderCircle,
  Hexagon,
  Zap,
  MemoryStick,
  type LucideIcon,
} from "lucide-react";
import "./App.css";

type TabId =
  | "android"
  | "native"
  | "windows"
  | "smali"
  | "java"
  | "hex"
  | "diff"
  | "split"
  | "console"
  | "frida"
  | "memscan"
  | "history"
  | "settings";

const TABS: { id: TabId; icon: LucideIcon; label: string }[] = [
  { id: "android", icon: Smartphone, label: "Android" },
  { id: "native", icon: Binary, label: "Native (.so)" },
  { id: "windows", icon: AppWindow, label: "Windows" },
  { id: "smali", icon: ScrollText, label: "Smali" },
  { id: "java", icon: Coffee, label: "Java" },
  { id: "hex", icon: Hash, label: "Hex" },
  { id: "diff", icon: GitCompare, label: "Diff" },
  { id: "split", icon: Split, label: "Split" },
  { id: "console", icon: Terminal, label: "Konsol" },
  { id: "frida", icon: Zap, label: "Frida" },
  { id: "memscan", icon: MemoryStick, label: "Bellek" },
  { id: "history", icon: History, label: "Geçmiş" },
  { id: "settings", icon: Settings, label: "Ayarlar" },
];

// Sidebar grupları — platforma/amaca göre mantıksal bölümleme.
const NAV_GROUPS: { title: string | null; ids: TabId[] }[] = [
  { title: "Android", ids: ["android", "native", "smali", "java", "split"] },
  { title: "Windows", ids: ["windows"] },
  { title: "Araçlar", ids: ["hex", "diff", "console", "frida", "memscan"] },
  { title: null, ids: ["history", "settings"] },
];
const NAV_FLAT: TabId[] = NAV_GROUPS.flatMap((g) => g.ids);
const tabDef = (id: TabId) => TABS.find((t) => t.id === id)!;

const baseName = (p: string) => p.split(/[/\\]/).pop() ?? p;
const PE_RE = /\.(exe|dll|sys|ocx|efi)$/i;

function App() {
  const [tab, setTab] = useState<TabId>("android");
  // Ziyaret edilen sekmeler bellekte tutulur (keep-alive): sekme değişince
  // önceki sekmenin durumu (decompile/arama/açık editör) sıfırlanmaz.
  const [mounted, setMounted] = useState<Set<TabId>>(() => new Set<TabId>(["android"]));
  const [apkPath, setApkPath] = useState<string | null>(null);
  const [profile, setProfile] = useState<AppProfile | null>(null);
  const [soPath, setSoPath] = useState<string | null>(null);
  const [symbols, setSymbols] = useState<SoSymbol[] | null>(null);
  const [nativeQuery, setNativeQuery] = useState("");
  const [hexPath, setHexPath] = useState<string | null>(null);
  const [pePath, setPePath] = useState<string | null>(null);
  const [diff, setDiff] = useState<ArchiveDiff | null>(null);
  const [diffPaths, setDiffPaths] = useState<{ a: string; b: string } | null>(null);
  const [tools, setTools] = useState<ToolReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const [accepted, setAccepted] = useState<boolean>(
    () => localStorage.getItem("byteforge.disclaimer") === "1"
  );

  const handleFile = async (path: string) => {
    setError(null);
    setProfile(null);
    setApkPath(null);
    setSymbols(null);
    setSoPath(null);
    setDiff(null);
    setPePath(null);
    setNativeQuery("");
    setBusy(true);
    try {
      if (path.toLowerCase().endsWith(".so")) {
        const syms = await invoke<SoSymbol[]>("list_so_symbols", { path });
        setSoPath(path);
        setSymbols(syms);
        setTab("native");
      } else if (PE_RE.test(path)) {
        setPePath(path);
        setTab("windows");
      } else {
        const prof = await invoke<AppProfile>("analyze_apk", { path });
        setApkPath(path);
        setProfile(prof);
        setTab("android");
      }
    } catch (e) {
      setError((e as Partial<ApiError>)?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  // CLI argümanı (açılışta) ve single-instance "open-file" olayı.
  useEffect(() => {
    invoke<string | null>("get_cli_file")
      .then((f) => {
        if (f) handleFile(f);
      })
      .catch(() => {});
    const unlisten = listen<string>("open-file", (e) => handleFile(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Aktif sekmeyi "mounted" kümesine ekle (ilk ziyarette bir kez).
  useEffect(() => {
    setMounted((prev) => {
      if (prev.has(tab)) return prev;
      const next = new Set(prev);
      next.add(tab);
      return next;
    });
  }, [tab]);

  // Klavye kısayolları: Alt+1..9/0 → sekme geçişi (10. sekme = 0).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || e.ctrlKey || e.metaKey) return;
      if (e.key >= "0" && e.key <= "9") {
        const idx = e.key === "0" ? 9 : Number(e.key) - 1;
        if (idx < NAV_FLAT.length) {
          e.preventDefault();
          setTab(NAV_FLAT[idx]);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    invoke<ToolReport>("check_tools").then(setTools).catch(console.error);
    const unlisten = getCurrentWebview().onDragDropEvent(async (event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") {
        setDragging(true);
        return;
      }
      if (p.type === "leave") {
        setDragging(false);
        return;
      }
      if (p.type !== "drop" || p.paths.length === 0) {
        setDragging(false);
        return;
      }
      setDragging(false);
      setError(null);
      setBusy(true);
      try {
        const first = p.paths[0];
        if (tab === "hex") {
          setHexPath(first);
        } else if (first.toLowerCase().endsWith(".so")) {
          const syms = await invoke<SoSymbol[]>("list_so_symbols", { path: first });
          setSoPath(first);
          setSymbols(syms);
          setNativeQuery("");
          setTab("native");
        } else if (PE_RE.test(first)) {
          setPePath(first);
          setTab("windows");
        } else if (p.paths.length >= 2) {
          const d = await invoke<ArchiveDiff>("diff_apks", {
            pathA: p.paths[0],
            pathB: p.paths[1],
          });
          setDiff(d);
          setDiffPaths({ a: p.paths[0], b: p.paths[1] });
          setTab("diff");
        } else {
          const prof = await invoke<AppProfile>("analyze_apk", { path: first });
          setApkPath(first);
          setProfile(prof);
          setTab("android");
        }
      } catch (e) {
        setError((e as Partial<ApiError>)?.message ?? String(e));
      } finally {
        setBusy(false);
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [tab]);

  // IL2CPP panelinden çağrılır: libil2cpp.so'yu Native sekmesine yükleyip
  // (isteğe bağlı) tıklanan sembol adını arama olarak açar — elle sürükleme yok.
  const loadedName = apkPath ? baseName(apkPath) : soPath ? baseName(soPath) : null;

  const empty = (msg: string) => <div className="panel-empty">{msg}</div>;

  // Sekme içeriğini `id`'ye göre üretir (aktif sekmeye değil) — keep-alive için.
  // Dosyaya bağlı ağır paneller dosya yoluyla `key`'lenir: sekme değişiminde
  // durum korunur, YENİ dosya yüklenince taze başlar.
  const renderTab = (id: TabId) => {
    switch (id) {
      case "android":
        return profile ? (
          <AndroidSection profile={profile} apkPath={apkPath} />
        ) : (
          empty("Bir .apk / .xapk / .apks dosyasını buraya sürükleyin.")
        );
      case "native":
        return soPath && symbols ? (
          <NativePanel key={soPath} soPath={soPath} symbols={symbols} initialQuery={nativeQuery} />
        ) : (
          empty("Bir .so (native kütüphane) dosyasını buraya sürükleyin.")
        );
      case "windows":
        return pePath ? (
          <PePanel key={pePath} pePath={pePath} />
        ) : (
          empty("Bir Windows .exe / .dll dosyasını buraya sürükleyin.")
        );
      case "smali":
        return apkPath ? (
          <SmaliWorkbench
            key={apkPath}
            apkPath={apkPath}
            runtime={profile?.runtime}
            onGoEngine={() => setTab("android")}
          />
        ) : (
          empty("Önce Profil sekmesinde bir APK yükleyin, sonra Smali'ye decode edin.")
        );
      case "java":
        return apkPath ? (
          <JavaWorkbench
            key={apkPath}
            apkPath={apkPath}
            runtime={profile?.runtime}
            onGoEngine={() => setTab("android")}
          />
        ) : (
          empty("Önce Profil sekmesinde bir APK yükleyin, sonra jadx ile Java'ya çevirin.")
        );
      case "hex":
        return hexPath ? (
          <HexEditor key={hexPath} path={hexPath} />
        ) : (
          empty("Herhangi bir ikili dosyayı (.so, .dex, .apk…) buraya sürükleyin.")
        );
      case "diff":
        return diff ? (
          <DiffView diff={diff} paths={diffPaths} />
        ) : (
          empty("İki APK'yı aynı anda buraya sürükleyin — dosya-düzeyi fark.")
        );
      case "split":
        return apkPath ? (
          <SplitPanel key={apkPath} apkPath={apkPath} />
        ) : (
          empty("Önce Profil sekmesinde bir XAPK/APKS yükleyin.")
        );
      case "console":
        return <ConsolePanel />;
      case "frida":
        return <FridaPanel />;
      case "memscan":
        return <MemScanner />;
      case "history":
        return <HistoryPanel />;
      case "settings":
        return <SettingsPanel />;
    }
  };

  return (
    <>
      {!accepted && (
        <Disclaimer
          onAccept={() => {
            localStorage.setItem("byteforge.disclaimer", "1");
            setAccepted(true);
          }}
        />
      )}

      <div className={`workbench ${dragging ? "dragging" : ""}`}>
        <aside className="sidebar">
          <div className="brand">
            <Hexagon className="brand-mark" size={23} strokeWidth={2.25} />
            Byte<span className="accent">Forge</span>
          </div>
          <nav>
            {NAV_GROUPS.map((g) => {
              const items = g.ids
                .map(tabDef)
                .filter((t) => t.id !== "native" || !!soPath);
              if (items.length === 0) return null;
              return (
                <div key={g.title ?? "meta"} className="nav-group">
                  {g.title && <span className="nav-group-title">{g.title}</span>}
                  {items.map((t) => {
                    const i = NAV_FLAT.indexOf(t.id);
                    return (
                      <button
                        key={t.id}
                        className={`nav-item ${tab === t.id ? "active" : ""}`}
                        onClick={() => setTab(t.id)}
                        title={i < 10 ? `${t.label} (Alt+${i === 9 ? 0 : i + 1})` : t.label}
                      >
                        <t.icon className="nav-icon" size={19} strokeWidth={1.85} />
                        <span>{t.label}</span>
                        {t.id === "native" &&
                          (profile?.runtime === "unity_il2cpp" ||
                            profile?.runtime === "flutter") && (
                            <span className="nav-badge">motor</span>
                          )}
                        {(t.id === "smali" || t.id === "java") &&
                          (profile?.runtime === "unity_il2cpp" ||
                            profile?.runtime === "unity_mono" ||
                            profile?.runtime === "flutter") && (
                            <span className="nav-shell">kabuk</span>
                          )}
                        {i >= 0 && i < 10 && <kbd className="nav-kbd">{i === 9 ? 0 : i + 1}</kbd>}
                      </button>
                    );
                  })}
                </div>
              );
            })}
          </nav>
        </aside>

        <div className="main">
          <header className="topbar">
            <span className="topbar-title">{TABS.find((t) => t.id === tab)?.label}</span>
            <span className="topbar-file">
              {busy ? (
                <>
                  <LoaderCircle size={14} className="spin" /> işleniyor…
                </>
              ) : loadedName ? (
                <>
                  <FolderOpen size={14} /> {loadedName}
                </>
              ) : (
                "dosya yüklenmedi"
              )}
            </span>
          </header>

          <section className="content">
            {error && <div className="error">Hata: {error}</div>}
            {TABS.filter((t) => mounted.has(t.id)).map((t) => (
              <div key={t.id} className="tab-pane" hidden={tab !== t.id}>
                {renderTab(t.id)}
              </div>
            ))}
          </section>

          <footer className="statusbar">
            {tools ? (
              tools.tools.map((t) => (
                <span key={t.name} className={`sb-tool ${t.available ? "ok" : "missing"}`}>
                  {t.available ? "●" : "○"} {t.name}
                </span>
              ))
            ) : (
              <span className="muted">araçlar denetleniyor…</span>
            )}
            <span className="sb-spacer" />
            <span className="muted">yalnızca yetkili/kendi paketlerinde kullanın</span>
          </footer>
        </div>
      </div>

      <Scratchpad />
    </>
  );
}

export default App;
