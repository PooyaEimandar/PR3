//! Exact arithmetic for the model. "Deterministic first, LLM second" (Roadmap §3): the
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
        chars: expression.chars().filter(|c| !c.is_whitespace()).collect(),
        pos: 0,
        depth: 0,
    };
    if parser.chars.is_empty() {
        return err("expression is empty");
    }
    let value = parser.expr()?;
    if let Some(c) = parser.peek() {
        return err(format!("unexpected '{c}'"));
    }
    if !value.is_finite() {
        return err("the result is not a finite number");
    }
    Ok(value)
}

/// Formats a result for the model: whole numbers without a fraction, others with up to
/// 12 significant digits.
pub fn format_number(value: f64) -> String {
    if value == value.trunc() && value.abs() < 1e15 {
        return format!("{value:.0}");
    }
    let digits = 12 - (value.abs().log10().floor() as i32 + 1);
    let decimals = digits.clamp(0, 12) as usize;
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
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
                value += self.term()?;
            } else if self.eat('-') {
                value -= self.term()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn term(&mut self) -> Result<f64, CalcError> {
        let mut value = self.power()?;
        loop {
            if self.eat('*') || self.eat('×') {
                value *= self.power()?;
            } else if self.eat('/') || self.eat('÷') {
                let d = self.power()?;
                if d == 0.0 {
                    return err("division by zero");
                }
                value /= d;
            } else if self.eat('%') {
                let d = self.power()?;
                if d == 0.0 {
                    return err("division by zero");
                }
                value %= d;
            } else {
                return Ok(value);
            }
        }
    }

    fn power(&mut self) -> Result<f64, CalcError> {
        let base = self.unary()?;
        if !self.eat('^') {
            return Ok(base);
        }
        let exponent = self.power()?;
        if exponent != exponent.trunc() || exponent.abs() > 1024.0 {
            return err("'^' needs a whole-number exponent between -1024 and 1024");
        }
        Ok(base.powi(exponent as i32))
    }

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
            self.primary()?
        };
        self.depth -= 1;
        Ok(value)
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

    fn number(&mut self) -> Result<f64, CalcError> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.pos += 1;
        }
        if self.peek().is_some_and(|c| c == 'e' || c == 'E') && self.pos > start {
            self.pos += 1;
            if self.peek().is_some_and(|c| c == '+' || c == '-') {
                self.pos += 1;
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if self.pos == start {
            return match self.peek() {
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
    }
}
