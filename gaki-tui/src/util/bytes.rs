//! Human-readable byte sizes in decimal units (`1.5 GB`), as the TUI shows them.

/// Format `num` bytes with a decimal unit, rounded to at most two decimals
/// (`0 B`, `512 B`, `1.5 kB`, `12.35 GB`).
pub fn convert(num: f64) -> String {
    const UNITS: [&str; 9] = ["B", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let sign = if num.is_sign_negative() { "-" } else { "" };
    let num = num.abs();
    if num < 1.0 {
        return format!("{sign}{num} B");
    }
    let exponent = ((num.log10() / 3.0).floor() as usize).min(UNITS.len() - 1);
    let scaled = num / 1000f64.powi(exponent as i32);
    // Round to two decimals, then print without trailing zeros.
    let rounded = (scaled * 100.0).round() / 100.0;
    format!("{sign}{rounded} {}", UNITS[exponent])
}

#[cfg(test)]
mod tests {
    use super::convert;

    #[test]
    fn formats_decimal_units() {
        assert_eq!(convert(0.0), "0 B");
        assert_eq!(convert(512.0), "512 B");
        assert_eq!(convert(1500.0), "1.5 kB");
        assert_eq!(convert(1000.0), "1 kB");
        assert_eq!(convert(12_345_678_901.0), "12.35 GB");
        assert_eq!(convert(-2_000_000.0), "-2 MB");
    }
}
