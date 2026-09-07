export function Disclaimer({ onAccept }: { onAccept: () => void }) {
  return (
    <div className="disclaimer-overlay">
      <div className="disclaimer-box">
        <h2>Kullanım Sorumluluğu</h2>
        <p>
          ByteForge yalnızca <b>sahibi olduğunuz</b> veya <b>test yetkiniz bulunan</b> paketlerde
          kullanılmalıdır: yetkili güvenlik testi, kendi uygulamanızı analiz, CTF ve eğitim.
          Telif korumalı yazılımların izinsiz değiştirilmesi/dağıtılması sizin
          sorumluluğunuzdadır.
        </p>
        <button onClick={onAccept}>Anladım, kabul ediyorum</button>
      </div>
    </div>
  );
}
