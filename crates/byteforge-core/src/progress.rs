//! Alt-süreç (jadx/apktool) çıktısından ilerleme (progress) ayrıştırma.
//! Saf metin; regex bağımlılığı yok.

/// Sayının sonundaki (virgül/boşlukları yok sayan) tam sayıyı okur.
fn trailing_number(s: &str) -> Option<u64> {
    let digits: String = s
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == ' ')
        .filter(|c| c.is_ascii_digit())
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().ok()
}

/// Baştaki (boşlukları atlayan) tam sayıyı okur.
fn leading_number(s: &str) -> Option<u64> {
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// jadx/apktool ilerleme satırından `(yüzde, mevcut, toplam)` çıkarır.
///
/// Desteklenen biçimler:
/// - `"... 1234 of 5678 ..."` → mevcut/toplam, yüzde hesaplanır
/// - `"... 45% ..."` → yalnızca yüzde (mevcut/toplam = 0)
///
/// Eşleşme yoksa None (normal log satırı).
pub fn parse_jadx_progress(line: &str) -> Option<(u32, u64, u64)> {
    let low = line.to_ascii_lowercase();

    // Biçim 1: "N of M"
    if let Some(pos) = low.find(" of ") {
        let cur = trailing_number(&low[..pos]);
        let tot = leading_number(&low[pos + 4..]);
        if let (Some(c), Some(t)) = (cur, tot) {
            if let Some(pct) = c.saturating_mul(100).checked_div(t) {
                return Some((pct.min(100) as u32, c, t));
            }
        }
    }

    // Biçim 2: "Z%"
    if let Some(pos) = low.find('%') {
        if let Some(p) = trailing_number(&low[..pos]) {
            return Some((p.min(100) as u32, 0, 0));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_x_of_y() {
        assert_eq!(
            parse_jadx_progress("INFO  - progress: 1234 of 5678"),
            Some((21, 1234, 5678))
        );
        assert_eq!(parse_jadx_progress("50 of 100"), Some((50, 50, 100)));
    }

    #[test]
    fn parses_percent() {
        assert_eq!(parse_jadx_progress("progress: 45%"), Some((45, 0, 0)));
        assert_eq!(
            parse_jadx_progress("decompiling... 100%"),
            Some((100, 0, 0))
        );
    }

    #[test]
    fn ignores_commas_in_numbers() {
        assert_eq!(
            parse_jadx_progress("12,345 of 28,000"),
            Some((44, 12345, 28000))
        );
    }

    #[test]
    fn plain_log_line_is_none() {
        assert_eq!(parse_jadx_progress("INFO  - loading classes"), None);
        assert_eq!(parse_jadx_progress("I: Baksmaling classes.dex"), None);
    }

    #[test]
    fn caps_at_100_and_avoids_div_zero() {
        assert_eq!(parse_jadx_progress("9999 of 100"), Some((100, 9999, 100)));
        assert_eq!(parse_jadx_progress("5 of 0"), None);
    }
}
