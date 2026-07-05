//! Parser input santai: "15k soto", "makan 15rb", "1.5jt kos", "+5jt gaji"
//! Prinsip: friction serendah mungkin. Nominal bisa di mana aja dalam kalimat.

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, PartialEq)]
pub struct ParsedInput {
    /// Nominal dalam Rupiah utuh
    pub amount: i64,
    /// Sisa teks sebagai catatan, e.g. "soto"
    pub note: String,
    /// true kalau diawali '+' → income
    pub is_income: bool,
}

static AMOUNT_RE: Lazy<Regex> = Lazy::new(|| {
    // contoh match: 15k, 15rb, 15ribu, 1.5jt, 1,5juta, 15000, 15.000
    Regex::new(r"(?i)(\d+(?:[.,]\d+)?)\s*(k|rb|ribu|jt|juta|m)?").unwrap()
});

pub fn parse(input: &str) -> Option<ParsedInput> {
    let trimmed = input.trim();
    let (is_income, text) = match trimmed.strip_prefix('+') {
        Some(rest) => (true, rest.trim()),
        None => (false, trimmed),
    };

    // Cari kandidat nominal pertama yang valid
    let caps = AMOUNT_RE.captures(text)?;
    let full_match = caps.get(0)?;
    let raw_num = caps.get(1)?.as_str().replace(',', ".");
    let suffix = caps.get(2).map(|m| m.as_str().to_lowercase());

    let base: f64 = if suffix.is_none() && raw_num.contains('.') {
        // "15.000" gaya Indonesia = 15000
        raw_num.replace('.', "").parse().ok()?
    } else {
        raw_num.parse().ok()?
    };

    let multiplier: f64 = match suffix.as_deref() {
        Some("k") | Some("rb") | Some("ribu") => 1_000.0,
        Some("jt") | Some("juta") => 1_000_000.0,
        Some("m") => 1_000_000.0, // "1m" umumnya maksudnya 1 juta di konteks lokal
        _ => 1.0,
    };

    let amount = (base * multiplier).round() as i64;
    if amount <= 0 {
        return None;
    }

    // Sisa teks di luar nominal jadi note
    let mut note = String::new();
    note.push_str(text[..full_match.start()].trim());
    if !note.is_empty() {
        note.push(' ');
    }
    note.push_str(text[full_match.end()..].trim());

    Some(ParsedInput {
        amount,
        note: note.trim().to_string(),
        is_income,
    })
}

/// Format 15000 → "Rp15.000"
pub fn format_rupiah(amount: i64) -> String {
    let s = amount.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    format!("Rp{}", out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let p = parse("15k soto").unwrap();
        assert_eq!(p.amount, 15_000);
        assert_eq!(p.note, "soto");
        assert!(!p.is_income);
    }

    #[test]
    fn parse_suffix_variants() {
        assert_eq!(parse("20rb bensin").unwrap().amount, 20_000);
        assert_eq!(parse("1.5jt kos").unwrap().amount, 1_500_000);
        assert_eq!(parse("kopi 15000").unwrap().amount, 15_000);
        assert_eq!(parse("15.000 nasi goreng").unwrap().amount, 15_000);
    }

    #[test]
    fn parse_income() {
        let p = parse("+5jt gaji").unwrap();
        assert_eq!(p.amount, 5_000_000);
        assert!(p.is_income);
    }

    #[test]
    fn parse_note_position() {
        let p = parse("makan siang 25k warteg").unwrap();
        assert_eq!(p.amount, 25_000);
        assert_eq!(p.note, "makan siang warteg");
    }

    #[test]
    fn format_ok() {
        assert_eq!(format_rupiah(1_500_000), "Rp1.500.000");
        assert_eq!(format_rupiah(500), "Rp500");
    }
}
