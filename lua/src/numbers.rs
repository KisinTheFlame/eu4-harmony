//! Bounded panel-entry numbers. Formatting never changes an entry's tooltip
//! or its slot, and a missing number is distinct from zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Integer,
    Decimal,
    Percent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Format {
    pub style: Style,
    pub decimals: u8,
}

impl Format {
    pub fn read(style: Option<&serde_json::Value>, decimals: Option<&serde_json::Value>) -> Result<Self, String> {
        let style = match style.and_then(serde_json::Value::as_str) {
            None if style.is_none() => Style::Integer,
            Some("integer") => Style::Integer,
            Some("decimal") => Style::Decimal,
            Some("percent") => Style::Percent,
            _ => return Err("entry.format is integer, decimal or percent".into()),
        };
        let decimals = match decimals {
            None => u8::from(style == Style::Decimal),
            Some(value) => value.as_u64().filter(|n| *n <= 2).ok_or("entry.decimals is 0, 1 or 2")? as u8,
        };
        if style == Style::Integer && decimals != 0 {
            return Err("an integer entry has no decimal places".into());
        }
        Ok(Self { style, decimals })
    }

    pub fn accepts(self, value: f64) -> bool {
        value.is_finite() && (self.style != Style::Percent || (value * 100.0).is_finite())
    }

    pub fn show(self, value: Option<f64>) -> String {
        let Some(value) = value.filter(|v| self.accepts(*v)) else { return String::new() };
        let mut value = if self.style == Style::Percent { value * 100.0 } else { value };
        let mut suffix = "";
        for next in ["K", "M", "B", "T"] {
            if value.abs() < 1000.0 {
                break;
            }
            value /= 1000.0;
            suffix = next;
        }
        let decimals = if suffix.is_empty() { self.decimals } else { self.decimals.min(1) };
        let rounded = (value * 10f64.powi(i32::from(decimals))).round() / 10f64.powi(i32::from(decimals));
        // Do not show a negative zero, or 1000K at a compact-unit boundary.
        let mut out = if rounded.abs() >= 1000.0 && suffix != "T" {
            let next = match suffix {
                "" => "K",
                "K" => "M",
                "M" => "B",
                "B" => "T",
                _ => unreachable!(),
            };
            format!("{:.*}{next}", usize::from(decimals), rounded / 1000.0)
        } else if rounded.abs() >= 1000.0 {
            format!("{value:.0e}{suffix}")
        } else {
            format!("{:.*}{suffix}", usize::from(decimals), if rounded == 0.0 { 0.0 } else { rounded })
        };
        if self.style == Style::Percent {
            out.push('%');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_zero_rounding_compact_boundaries_and_percent_are_distinct() {
        let integer = Format::default();
        let decimal = Format { style: Style::Decimal, decimals: 1 };
        let percent = Format { style: Style::Percent, decimals: 1 };
        assert_eq!(integer.show(None), "");
        assert_eq!(integer.show(Some(0.0)), "0");
        assert_eq!(integer.show(Some(12.6)), "13");
        assert_eq!(decimal.show(Some(-0.01)), "0.0");
        assert_eq!(decimal.show(Some(12.36)), "12.4");
        assert_eq!(decimal.show(Some(1234.0)), "1.2K");
        assert_eq!(integer.show(Some(999_999.0)), "1M");
        assert_eq!(percent.show(Some(0.125)), "12.5%");
        assert!(!percent.accepts(f64::MAX));
        assert!(!integer.accepts(f64::INFINITY));
    }
}
