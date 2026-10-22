//! Exact arithmetic for the model. "Deterministic first, LLM second" (docs Sec. 3): the
//! model decides *what* to compute; this code computes it.
//!
//! Grammar: `+ - * / % ^`, parentheses, unary minus, decimals and exponents
//! (`1.5e3`). `^` takes an integer exponent and is right-associative.

use std::fmt;

const MAX_LEN: usize = 256;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalcError(String);

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn err<T>(msg: impl Into<String>) -> Result<T, CalcError> {
    Err(CalcError(msg.into()))
}

/// Evaluates `expression`, e.g. `"12 * (3 + 4)"`.
pub fn evaluate(expression: &str) -> Result<f64, CalcError> {
    if expression.len() > MAX_LEN {
        return err(format!("expression is longer than {MAX_LEN} characters"));
    }
    let mut parser = Parser {
        chars: expression.chars().collect(),
        pos: 0,
        depth: 0,
    };
    if parser.peek().is_none() {
        return err("expression is empty");
    }
    let value = parser.expr()?;
    if let Some(c) = parser.peek() {
        return err(format!("unexpected '{c}'"));
    }
    Ok(value)
}

/// Formats a result for the model: whole numbers without a fraction, others with up to
/// 12 significant digits, and very large or small magnitudes in scientific notation.
pub fn format_number(value: f64) -> String {
    if value == 0.0 {
        // Also turns -0 into 0.
        return "0".to_string();
    }
    let magnitude = value.abs();
    // Whole numbers print exactly while f64 holds every integer (up to 2^53).
    if value == value.trunc() && magnitude < 9_007_199_254_740_992.0 {
        return format!("{value:.0}");
    }
    if !(1e-9..1e15).contains(&magnitude) {
        let text = format!("{value:.11e}");
        return match text.split_once('e') {
            Some((mantissa, exponent)) => {
                let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
                format!("{mantissa}e{exponent}")
            }
            None => text,
        };
    }
    let decimals = (11 - magnitude.log10().floor() as i32).clamp(0, 20) as usize;
    let text = format!("{value:.decimals$}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Every intermediate result must be a finite number.
fn finite(value: f64) -> Result<f64, CalcError> {
    if value.is_finite() {
        Ok(value)
    } else {
        err("the result is too large")
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
}

impl Parser {
    /// The next character that is not white space.
    fn peek(&mut self) -> Option<char> {
        while self.chars.get(self.pos).is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
        self.chars.get(self.pos).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Result<f64, CalcError> {
        let mut value = self.term()?;
        loop {
            if self.eat('+') {
                value = finite(value + self.term()?)?;
            } else if self.eat('-') {
                value = finite(value - self.term()?)?;
            } else {
                return Ok(value);
            }
        }
    }

    fn term(&mut self) -> Result<f64, CalcError> {
        let mut value = self.unary()?;
        loop {
            if self.eat('*') || self.eat('×') {
                value = finite(value * self.unary()?)?;
            } else if self.eat('/') || self.eat('÷') {
                let d = self.unary()?;
                if d == 0.0 {
                    return err("division by zero");
                }
                value = finite(value / d)?;
            } else if self.eat('%') {
                let d = self.unary()?;
                if d == 0.0 {
                    return err("division by zero");
                }
                value %= d;
            } else {
                return Ok(value);
            }
        }
    }

    /// Sign binds more loosely than `^`, as in mathematics: `-2^2` is -4.
    fn unary(&mut self) -> Result<f64, CalcError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return err("expression is nested too deeply");
        }
        let value = if self.eat('-') {
            -self.unary()?
        } else if self.eat('+') {
            self.unary()?
        } else {
            self.power()?
        };
        self.depth -= 1;
        Ok(value)
    }

    fn power(&mut self) -> Result<f64, CalcError> {
        let base = self.primary()?;
        if !self.eat('^') {
            return Ok(base);
        }
        // Right-associative, and the exponent may carry a sign: 2^-1 is 0.5.
        let exponent = self.unary()?;
        if exponent != exponent.trunc() || exponent.abs() > 1024.0 {
            return err("'^' needs a whole-number exponent between -1024 and 1024");
        }
        finite(base.powi(exponent as i32))
    }

    fn primary(&mut self) -> Result<f64, CalcError> {
        if self.eat('(') {
            let value = self.expr()?;
            if !self.eat(')') {
                return err("missing ')'");
            }
            return Ok(value);
        }
        self.number()
    }

    /// Digits, a point and an exponent, with no spaces inside: `1 2` is not 12.
    fn number(&mut self) -> Result<f64, CalcError> {
        let at = |p: &Self, i: usize| p.chars.get(i).copied();
        let start = match self.peek() {
            Some(_) => self.pos,
            None => return err("expression ends early"),
        };
        while at(self, self.pos).is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.pos += 1;
        }
        if at(self, self.pos).is_some_and(|c| c == 'e' || c == 'E') && self.pos > start {
            self.pos += 1;
            if at(self, self.pos).is_some_and(|c| c == '+' || c == '-') {
                self.pos += 1;
            }
            while at(self, self.pos).is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if self.pos == start {
            return match at(self, self.pos) {
                Some(c) => err(format!("unexpected '{c}'")),
                None => err("expression ends early"),
            };
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>()
            .or_else(|_| err(format!("'{text}' is not a number")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc(e: &str) -> String {
        evaluate(e)
            .map(format_number)
            .unwrap_or_else(|x| x.to_string())
    }

    #[test]
    fn arithmetic_with_precedence() {
        assert_eq!(calc("1234 * 5678"), "7006652");
        assert_eq!(calc("2 + 3 * 4"), "14");
        assert_eq!(calc("(2 + 3) * 4"), "20");
        assert_eq!(calc("2 ^ 3 ^ 2"), "512");
        assert_eq!(calc("-3 + 5"), "2");
        assert_eq!(calc("10 / 4"), "2.5");
        assert_eq!(calc("1/3"), "0.333333333333");
        assert_eq!(calc("7 % 3"), "1");
        assert_eq!(calc("1.5e3 + 1"), "1501");
        assert_eq!(calc("6 × 7 ÷ 2"), "21");
        assert_eq!(calc("-2^2"), "-4");
        assert_eq!(calc("2^-1"), "0.5");
        assert_eq!(calc("(-2)^2"), "4");
        assert_eq!(calc("0 * -1"), "0");
        assert_eq!(calc("1.23456789e-5"), "0.0000123456789");
        assert_eq!(calc("1e-12"), "1e-12");
        assert_eq!(calc("2^52"), "4503599627370496");
        assert_eq!(calc("2^60"), "1.15292150461e18");
    }

    #[test]
    fn errors_explain_themselves() {
        assert_eq!(calc("1 / 0"), "division by zero");
        assert_eq!(calc("2 +"), "expression ends early");
        assert_eq!(calc("(1 + 2"), "missing ')'");
        assert_eq!(calc("2 $ 3"), "unexpected '$'");
        assert_eq!(calc(""), "expression is empty");
        assert!(calc("2 ^ 0.5").contains("whole-number"));
        assert!(calc(&"(".repeat(100)).contains("nested"));
        assert!(calc(&"1+".repeat(200)).contains("longer"));
        assert_eq!(calc("1 2 + 3"), "unexpected '2'");
        assert!(calc("1/10^309").contains("too large"));
        assert!(calc("1e308 * 10").contains("too large"));
        assert_eq!(calc("   "), "expression is empty");
    }
}
