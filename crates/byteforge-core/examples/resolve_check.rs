// Resolver + TSV/arama uçtan uca doğrulama (Tauri komut yolunun aynısı).
use byteforge_core::il2cpp_resolve::resolve_methods;
fn main() {
    let dir = std::env::args().nth(1).expect("cache dizini ver");
    let md = std::fs::read(format!("{dir}/global-metadata.dat")).unwrap();
    let so = std::fs::read(format!("{dir}/libil2cpp-arm64-v8a.so")).unwrap();
    let t = std::time::Instant::now();
    let mut r = resolve_methods(&md, &so).expect("çözüm başarısız");
    println!("{} | {:.2}s", r.note, t.elapsed().as_secs_f64());
    // Komuttaki gibi TSV yaz.
    r.methods.sort_by(|a, b| a.name.cmp(&b.name));
    let tsv: String = r.methods.iter()
        .map(|m| format!("{:x}\t{}\t{}\t{}\n", m.rva, m.type_name, m.method_name, m.image))
        .collect();
    std::fs::write("/tmp/_resolved.tsv", &tsv).unwrap();
    // Arama simülasyonu.
    let search = |q: &str| {
        let ql = q.to_lowercase();
        let hits: Vec<_> = tsv.lines().filter(|l| {
            let mut it = l.split('\t'); let rva=it.next().unwrap();
            let ty=it.next().unwrap(); let mn=it.next().unwrap();
            format!("{ty}.{mn}").to_lowercase().contains(&ql).then_some(()).map(|_| (rva,ty,mn)).is_some()
        }).take(6).collect();
        println!("\narama '{q}': {} sonuç", hits.len());
        for l in hits { let p:Vec<_>=l.split('\t').collect(); println!("  0x{}  {}.{}", p[0].to_uppercase(), p[1], p[2]); }
    };
    search("ActiveDurationRatio");
    search("PowerConfig.GetDuration");
    search("IsSkinOwned");
    search("GetCurrency");
}
