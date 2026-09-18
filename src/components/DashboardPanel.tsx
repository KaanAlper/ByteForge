import {
  UploadCloud,
  AppWindow,
  Smartphone,
  MemoryStick,
  ShieldAlert,
  ArrowRight,
  Zap,
  FolderOpen,
  Binary,
  CheckCircle2,
} from "lucide-react";

interface DashboardPanelProps {
  onPickFile: () => void;
  onNavigate: (tab: string) => void;
  activeFile: { name: string; type: "pe" | "apk" | "so"; path: string } | null;
  busy: boolean;
  dragging: boolean;
}

export function DashboardPanel({
  onPickFile,
  onNavigate,
  activeFile,
  busy,
  dragging,
}: DashboardPanelProps) {
  return (
    <div className="dashboard-container">
      {/* Aktif Dosya Varsa Hızlı Özet Kartı */}
      {activeFile && (
        <div className="dash-active-card">
          <div className="dash-active-left">
            <CheckCircle2 size={18} className="dash-icon-ok" />
            <div>
              <div className="dash-active-title">Yüklü Dosya: {activeFile.name}</div>
              <div className="dash-active-path mono">{activeFile.path}</div>
            </div>
          </div>
          <div className="dash-active-actions">
            {activeFile.type === "pe" && (
              <button className="primary-btn" onClick={() => onNavigate("windows")}>
                PE Analizine Git <ArrowRight size={14} />
              </button>
            )}
            {activeFile.type === "apk" && (
              <button className="primary-btn" onClick={() => onNavigate("android")}>
                APK Modlamaya Git <ArrowRight size={14} />
              </button>
            )}
            {activeFile.type === "so" && (
              <button className="primary-btn" onClick={() => onNavigate("native")}>
                Native (.so) Analizine Git <ArrowRight size={14} />
              </button>
            )}
            <button className="dash-btn-secondary" onClick={() => onNavigate("hex")}>
              Hex
            </button>
            <button className="dash-btn-secondary" onClick={() => onNavigate("yara")}>
              YARA
            </button>
          </div>
        </div>
      )}

      {/* Akıllı Dropzone (Tıklandığında Dosya Seçici Açar) */}
      <div
        className={`dash-dropzone ${dragging ? "dragging" : ""} ${busy ? "busy" : ""}`}
        onClick={onPickFile}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onPickFile();
          }
        }}
      >
        <div className="dash-dropzone-icon">
          <UploadCloud size={36} />
        </div>
        <div className="dash-dropzone-text">
          <h3>
            {busy
              ? "Dosya çözümleniyor..."
              : "Dosyayı buraya sürükleyin veya seçmek için tıklayın"}
          </h3>
          <p className="muted">
            Otomatik format algılama: .exe, .dll, .apk, .xapk, .apks, .so
          </p>
        </div>
        <button
          type="button"
          className="dash-browse-btn"
          onClick={(e) => {
            e.stopPropagation();
            onPickFile();
          }}
          disabled={busy}
        >
          <FolderOpen size={15} /> Dosya Seç
        </button>
      </div>

      {/* Hızlı İşlem Kartları */}
      <div className="dash-section-title">Hızlı Başlangıç ve Otomasyon</div>
      <div className="dash-cards-grid">
        {/* Windows Kartı */}
        <div className="dash-card">
          <div className="dash-card-header">
            <div className="dash-card-icon">
              <AppWindow size={20} />
            </div>
            <span className="dash-badge">Windows</span>
          </div>
          <h4>Windows Program Analizi</h4>
          <p className="dash-card-desc">
            PE başlıkları, export ve section analizi, API bağımlılıkları, şüpheli string tespiti
            ve opcode tabanlı yama tezgahı.
          </p>
          <div className="dash-card-footer">
            <button className="dash-action-btn" onClick={onPickFile}>
              <FolderOpen size={14} /> EXE / DLL Aç
            </button>
            <button
              className="dash-text-btn"
              onClick={() => onNavigate("windows")}
            >
              Doğrudan Panele Git <ArrowRight size={13} />
            </button>
          </div>
        </div>

        {/* Android Kartı */}
        <div className="dash-card">
          <div className="dash-card-header">
            <div className="dash-card-icon">
              <Smartphone size={20} />
            </div>
            <span className="dash-badge">Android</span>
          </div>
          <h4>Mobil & IL2CPP Modlama</h4>
          <p className="dash-card-desc">
            APK manifesti, izinler, IL2CPP sembol çözücü, ekonomi ve reklam hedef
            puanlaması, Smali düzenleyici ve yüzen mod menüsü enjeksiyonu.
          </p>
          <div className="dash-card-footer">
            <button className="dash-action-btn" onClick={onPickFile}>
              <FolderOpen size={14} /> APK Aç
            </button>
            <button
              className="dash-text-btn"
              onClick={() => onNavigate("android")}
            >
              Doğrudan Panele Git <ArrowRight size={13} />
            </button>
          </div>
        </div>

        {/* Canlı Bellek (Cheat Engine) */}
        <div className="dash-card">
          <div className="dash-card-header">
            <div className="dash-card-icon">
              <MemoryStick size={20} />
            </div>
            <span className="dash-badge">Bellek</span>
          </div>
          <h4>Canlı Süreç Bellek Tarayıcısı</h4>
          <p className="dash-card-desc">
            Çalışan süreçleri bağlama, tam ve fuzzy değer taraması, adres sabitleme/dondurma ve
            pointer tarama operasyonları.
          </p>
          <div className="dash-card-footer">
            <button
              className="dash-action-btn"
              onClick={() => onNavigate("memscan")}
            >
              <Zap size={14} /> Bellek Tarayıcıyı Aç
            </button>
          </div>
        </div>

        {/* Güvenlik & YARA Taraması */}
        <div className="dash-card">
          <div className="dash-card-header">
            <div className="dash-card-icon">
              <ShieldAlert size={20} />
            </div>
            <span className="dash-badge">Analiz</span>
          </div>
          <h4>YARA & Tehdit Analizi</h4>
          <p className="dash-card-desc">
            Statik dosyalarda gömülü URL, IP, lisans mekanizması, packer (UPX vb.) ve anti-debug
            imzalarını tarama.
          </p>
          <div className="dash-card-footer">
            <button
              className="dash-action-btn"
              onClick={() => onNavigate("yara")}
            >
              <Binary size={14} /> YARA Panelini Aç
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
