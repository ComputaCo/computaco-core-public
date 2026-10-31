//! Arithmetic for computed parameters: a loan payment, a resistor from Ohm's law, a
//! percentage change. Fixed-point decimals in an `i128` (twelve places), so the value a
//! check names is the same on every platform, with no floating point anywhere.
use std::collections::BTreeMap;

const PLACES: u32 = 12;
const ONE: i128 = 10i128.pow(PLACES);

/// A decimal with twelve places, as an integer number of 10⁻¹² units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Decimal(i128);

impl Decimal {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().replace(',', "");
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest.to_owned()),
            None => (false, text),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((&digits, ""));
        if whole.is_empty() && fraction.is_empty()
            || !whole
                .chars()
                .chain(fraction.chars())
                .all(|c| c.is_ascii_digit())
            || fraction.len() > PLACES as usize
        {
            return None;
        }
        let whole: i128 = if whole.is_empty() {
            0
        } else {
            whole.parse().ok()?
        };
        let mut padded = fraction.to_owned();
        padded.extend(std::iter::repeat_n('0', PLACES as usize - fraction.len()));
        let units = whole
            .checked_mul(ONE)?
            .checked_add(padded.parse::<i128>().ok()?)?;
        Some(Self(if negative { -units } else { units }))
    }
    fn mul(self, o: Self) -> Option<Self> {
        Some(Self(round_div(self.0.checked_mul(o.0)?, ONE)))
    }
    fn div(self, o: Self) -> Option<Self> {
        (o.0 != 0).then_some(())?;
        Some(Self(round_div(self.0.checked_mul(ONE)?, o.0)))
    }
    /// Half away from zero, to `places` decimals.
    fn round(self, places: u32) -> Self {
        let unit = 10i128.pow(PLACES - places.min(PLACES));
        Self(round_div(self.0, unit) * unit)
    }
    fn floor(self) -> Self {
        Self(self.0.div_euclid(ONE) * ONE)
    }
    fn ceil(self) -> Self {
        Self(-(-self.0).div_euclid(ONE) * ONE)
    }
    /// The value with exactly `places` decimals (rounded), or with no trailing zeros
    /// when `places` is `None`; `grouping` puts commas between thousands.
    pub fn format(self, places: Option<u32>, grouping: bool) -> String {
        let value = places.map_or(self, |p| self.round(p));
        let negative = value.0 < 0;
        let units = value.0.unsigned_abs();
        let whole = (units / ONE as u128).to_string();
        let mut fraction = format!("{:0width$}", units % ONE as u128, width = PLACES as usize);
        match places {
            Some(p) => fraction.truncate(p as usize),
            None => fraction = fraction.trim_end_matches('0').to_owned(),
        }
        let whole = if grouping {
            let mut out = String::new();
            for (i, c) in whole.chars().enumerate() {
                if i > 0 && (whole.len() - i).is_multiple_of(3) {
                    out.push(',');
                }
                out.push(c);
            }
            out
        } else {
            whole
        };
        let sign = if negative && (units != 0) { "-" } else { "" };
        if fraction.is_empty() {
            format!("{sign}{whole}")
        } else {
            format!("{sign}{whole}.{fraction}")
        }
    }
}
/// Integer division rounding half away from zero.
fn round_div(n: i128, d: i128) -> i128 {
    let (q, r) = (n / d, n % d);
    if 2 * r.abs() >= d.abs() {
        q + if (n < 0) == (d < 0) { 1 } else { -1 }
    } else {
        q
    }
}

/// Evaluate `text` with earlier parameters as variables. Supports `+ - * /`, unary
/// minus, parentheses, and `round(x, places)`, `floor(x)`, `ceil(x)`, `abs(x)`,
/// `min(a, b, …)`, `max(a, b, …)`, `pow(x, k)` for a whole `k` (negative allowed).
pub fn evaluate(text: &str, values: &BTreeMap<String, String>) -> Result<Decimal, String> {
    let mut parser = Parser {
        text: text.as_bytes(),
        at: 0,
        values,
    };
    let value = parser.sum()?;
    parser.space();
    if parser.at != parser.text.len() {
        return Err(format!("unexpected {:?} in {text:?}", &text[parser.at..]));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
    values: &'a BTreeMap<String, String>,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.space();
        let hit = self.text.get(self.at) == Some(&c);
        self.at += usize::from(hit);
        hit
    }
    fn sum(&mut self) -> Result<Decimal, String> {
        let mut value = self.product()?;
        loop {
            if self.eat(b'+') {
                value = Decimal(value.0.checked_add(self.product()?.0).ok_or("overflow")?);
            } else if self.eat(b'-') {
                value = Decimal(value.0.checked_sub(self.product()?.0).ok_or("overflow")?);
            } else {
                return Ok(value);
            }
        }
    }
    fn product(&mut self) -> Result<Decimal, String> {
        let mut value = self.unary()?;
        loop {
            if self.eat(b'*') {
                value = value.mul(self.unary()?).ok_or("overflow")?;
            } else if self.eat(b'/') {
                value = value
                    .div(self.unary()?)
                    .ok_or("division by zero or overflow")?;
            } else {
                return Ok(value);
            }
        }
    }
    fn unary(&mut self) -> Result<Decimal, String> {
        if self.eat(b'-') {
            return Ok(Decimal(-self.unary()?.0));
        }
        self.atom()
    }
    fn atom(&mut self) -> Result<Decimal, String> {
        self.space();
        if self.eat(b'(') {
            let value = self.sum()?;
            return if self.eat(b')') {
                Ok(value)
            } else {
                Err("missing )".into())
            };
        }
        let start = self.at;
        let number = |c: &u8| c.is_ascii_digit() || *c == b'.';
        if self.text.get(self.at).is_some_and(number) {
            while self.text.get(self.at).is_some_and(number) {
                self.at += 1;
            }
            let digits = std::str::from_utf8(&self.text[start..self.at]).unwrap_or_default();
            return Decimal::parse(digits).ok_or_else(|| format!("bad number {digits:?}"));
        }
        let word = |c: &u8| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'.';
        while self.text.get(self.at).is_some_and(word) {
            self.at += 1;
        }
        let name = std::str::from_utf8(&self.text[start..self.at])
            .unwrap_or_default()
            .to_owned();
        if name.is_empty() {
            return Err(format!("expected a value at {}", self.at));
        }
        if !self.eat(b'(') {
            let text = self
                .values
                .get(&name)
                .ok_or_else(|| format!("no parameter named {name:?}"))?;
            return Decimal::parse(text)
                .ok_or_else(|| format!("{name} = {text:?} is not a number"));
        }
        let mut args = vec![self.sum()?];
        while self.eat(b',') {
            args.push(self.sum()?);
        }
        if !self.eat(b')') {
            return Err(format!("missing ) after {name}("));
        }
        let whole = |d: Decimal| -> Result<i64, String> {
            (d.0 % ONE == 0)
                .then_some((d.0 / ONE) as i64)
                .ok_or_else(|| format!("{name} needs a whole number"))
        };
        match (name.as_str(), args.as_slice()) {
            ("round", [x]) => Ok(x.round(0)),
            ("round", [x, places]) => {
                Ok(x.round(u32::try_from(whole(*places)?).map_err(|_| "bad places")?))
            }
            ("floor", [x]) => Ok(x.floor()),
            ("ceil", [x]) => Ok(x.ceil()),
            ("abs", [x]) => Ok(Decimal(x.0.abs())),
            ("min", [first, ..]) => Ok(*args.iter().min().unwrap_or(first)),
            ("max", [first, ..]) => Ok(*args.iter().max().unwrap_or(first)),
            ("pow", [x, k]) => {
                let k = whole(*k)?;
                let mut out = Decimal(ONE);
                for _ in 0..k.unsigned_abs().min(10_000) {
                    out = out.mul(*x).ok_or("overflow")?;
                }
                if k < 0 {
                    out = Decimal(ONE).div(out).ok_or("division by zero")?;
                }
                Ok(out)
            }
            _ => Err(format!(
                "unknown function {name} with {} arguments",
                args.len()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn eval(text: &str, vars: &[(&str, &str)]) -> String {
        let values = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        evaluate(text, &values).unwrap().format(None, false)
    }
    #[test]
    fn arithmetic_is_exact_and_rounds_half_away_from_zero() {
        assert_eq!(eval("1 + 2 * 3", &[]), "7");
        assert_eq!(eval("(1 + 2) * 3 - -1", &[]), "10");
        assert_eq!(eval("10 / 4", &[]), "2.5");
        assert_eq!(eval("round(2.345, 2)", &[]), "2.35");
        assert_eq!(eval("round(-2.345, 2)", &[]), "-2.35");
        assert_eq!(eval("floor(-1.5) + ceil(1.2)", &[]), "0");
        assert_eq!(eval("max(a, 3, b)", &[("a", "1"), ("b", "4.5")]), "4.5");
        assert_eq!(eval("pow(2, -2)", &[]), "0.25");
        // A record's fields are parameters too.
        assert_eq!(eval("item.cents / 100", &[("item.cents", "1250")]), "12.5");
    }
    #[test]
    fn a_loan_payment_matches_the_spreadsheet() {
        // PMT(5%/12, 60, -25000) = 471.78 in any spreadsheet.
        let p = eval(
            "round(principal * (rate / 1200) / (1 - pow(1 + rate / 1200, -months)), 2)",
            &[("principal", "25000"), ("rate", "5"), ("months", "60")],
        );
        assert_eq!(p, "471.78");
    }
    #[test]
    fn formatting_groups_and_pads() {
        let d = Decimal::parse("1234567.5").unwrap();
        assert_eq!(d.format(Some(2), true), "1,234,567.50");
        assert_eq!(d.format(Some(0), false), "1234568");
        assert_eq!(
            Decimal::parse("-0.004").unwrap().format(Some(2), false),
            "0.00"
        );
        assert!(evaluate("1 / 0", &BTreeMap::new()).is_err());
        assert!(evaluate("x + 1", &BTreeMap::new()).is_err());
    }
}
