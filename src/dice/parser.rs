use std::fmt;

use crate::dice::kind::DieKind;
use crate::dice::options::Options;
use crate::dice::roll::{DiceRoll, DiceTerm};

/// Failures returned by [`DiceRoll::parse`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    /// Input was empty or whitespace-only.
    Empty,
    /// A `d` was followed by a number that doesn't match a supported [`DieKind`].
    UnknownDieSides(u32),
    /// A term used a count of zero (e.g. `0d6`).
    ZeroCount,
    /// A numeric literal or accumulated adjustment did not fit in `u32`/`i32`.
    Overflow,
    /// Generic syntax error at `position` (byte offset) with a short reason.
    Malformed { position: usize, reason: &'static str },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty dice expression"),
            ParseError::UnknownDieSides(sides) => write!(f, "no die has {sides} sides"),
            ParseError::ZeroCount => write!(f, "die count must be at least 1"),
            ParseError::Overflow => write!(f, "numeric literal too large"),
            ParseError::Malformed { position, reason } => {
                write!(f, "malformed expression at byte {position}: {reason}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl DiceRoll {
    /// Parses a dice expression like `3d6+2` or `1d20-1d4+5`.
    ///
    /// Grammar: one or more `NdK` terms (count optional, defaults to 1)
    /// and/or integer adjustments, separated by `+` or `-`. Whitespace is
    /// ignored. Both `d` and `D` are accepted. Supported sides: 4, 6, 8, 10,
    /// 12, 20, 100.
    ///
    /// # Examples
    ///
    /// ```
    /// use bevy_dice::dice::DiceRoll;
    ///
    /// let roll = DiceRoll::parse("3d6+2").unwrap();
    /// assert_eq!(roll.terms.len(), 1);
    /// assert_eq!(roll.adjustment, 2);
    /// ```
    pub fn parse(input: &str) -> Result<DiceRoll, ParseError> {
        let mut lexer = Lexer::new(input);
        lexer.skip_whitespace();
        if lexer.is_done() {
            return Err(ParseError::Empty);
        }

        let mut terms = Vec::new();
        let mut adjustment: i32 = 0;
        let mut negate = false;

        loop {
            lexer.skip_whitespace();
            let token = lexer.read_token()?;
            match token {
                Token::Dice { count, sides } => {
                    let kind = DieKind::from_sides(sides).ok_or(ParseError::UnknownDieSides(sides))?;
                    if count == 0 {
                        return Err(ParseError::ZeroCount);
                    }
                    terms.push(DiceTerm { count, kind, negate, options: Options::default() });
                }
                Token::Number(value) => {
                    let signed = if negate { -(value as i32) } else { value as i32 };
                    adjustment = adjustment.checked_add(signed).ok_or(ParseError::Overflow)?;
                }
            }

            lexer.skip_whitespace();
            if lexer.is_done() {
                break;
            }
            negate = lexer.read_sign()?;
        }

        Ok(DiceRoll { terms, adjustment })
    }
}

struct Lexer<'a> {
    source: &'a [u8],
    cursor: usize,
}

enum Token {
    Dice { count: u32, sides: u32 },
    Number(u32),
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { source: input.as_bytes(), cursor: 0 }
    }

    fn is_done(&self) -> bool {
        self.cursor >= self.source.len()
    }

    fn peek(&self) -> Option<u8> {
        self.source.get(self.cursor).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.cursor += 1;
        Some(byte)
    }

    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.peek() {
            if byte.is_ascii_whitespace() {
                self.cursor += 1;
            } else {
                break;
            }
        }
    }

    // true for `-`, false for `+`.
    fn read_sign(&mut self) -> Result<bool, ParseError> {
        let position = self.cursor;
        match self.bump() {
            Some(b'+') => Ok(false),
            Some(b'-') => Ok(true),
            _ => Err(ParseError::Malformed { position, reason: "expected '+' or '-'" }),
        }
    }

    fn read_digits(&mut self) -> Result<Option<u32>, ParseError> {
        let mut value: u32 = 0;
        let mut any = false;
        while let Some(byte) = self.peek() {
            if byte.is_ascii_digit() {
                value = value
                    .checked_mul(10)
                    .and_then(|v| v.checked_add((byte - b'0') as u32))
                    .ok_or(ParseError::Overflow)?;
                any = true;
                self.cursor += 1;
            } else {
                break;
            }
        }
        Ok(if any { Some(value) } else { None })
    }

    fn read_token(&mut self) -> Result<Token, ParseError> {
        let position = self.cursor;
        let leading = self.read_digits()?;
        let next = self.peek();
        if matches!(next, Some(b'd' | b'D')) {
            self.cursor += 1;
            let sides = self.read_digits()?.ok_or(ParseError::Malformed {
                position: self.cursor,
                reason: "expected digits after 'd'",
            })?;
            Ok(Token::Dice { count: leading.unwrap_or(1), sides })
        } else {
            let value = leading.ok_or(ParseError::Malformed {
                position,
                reason: "expected number or dice term",
            })?;
            Ok(Token::Number(value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_die() {
        let roll = DiceRoll::parse("d20").unwrap();
        assert_eq!(roll.terms.len(), 1);
        assert_eq!(roll.terms[0].count, 1);
        assert_eq!(roll.terms[0].kind, DieKind::D20);
        assert_eq!(roll.adjustment, 0);
    }

    #[test]
    fn parses_count_and_die() {
        let roll = DiceRoll::parse("3d6").unwrap();
        assert_eq!(roll.terms[0].count, 3);
        assert_eq!(roll.terms[0].kind, DieKind::D6);
    }

    #[test]
    fn parses_with_positive_adjustment() {
        let roll = DiceRoll::parse("3d6+2").unwrap();
        assert_eq!(roll.terms[0].count, 3);
        assert_eq!(roll.adjustment, 2);
    }

    #[test]
    fn parses_with_negative_adjustment() {
        let roll = DiceRoll::parse("d20-2").unwrap();
        assert_eq!(roll.terms[0].kind, DieKind::D20);
        assert_eq!(roll.adjustment, -2);
    }

    #[test]
    fn parses_compound_expression() {
        let roll = DiceRoll::parse("2d6+1d4+3").unwrap();
        assert_eq!(roll.terms.len(), 2);
        assert_eq!(roll.terms[0].kind, DieKind::D6);
        assert_eq!(roll.terms[0].count, 2);
        assert_eq!(roll.terms[1].kind, DieKind::D4);
        assert_eq!(roll.terms[1].count, 1);
        assert_eq!(roll.adjustment, 3);
    }

    #[test]
    fn parses_negated_dice_term() {
        let roll = DiceRoll::parse("1d20-1d4").unwrap();
        assert_eq!(roll.terms.len(), 2);
        assert!(!roll.terms[0].negate);
        assert!(roll.terms[1].negate);
    }

    #[test]
    fn accepts_whitespace_and_uppercase_d() {
        let roll = DiceRoll::parse("  2D6  +  1D4  + 3 ").unwrap();
        assert_eq!(roll.terms.len(), 2);
        assert_eq!(roll.adjustment, 3);
    }

    #[test]
    fn collapses_multiple_adjustments() {
        let roll = DiceRoll::parse("d6+2+3-1").unwrap();
        assert_eq!(roll.adjustment, 4);
    }

    #[test]
    fn rejects_empty_input() {
        assert_eq!(DiceRoll::parse(""), Err(ParseError::Empty));
        assert_eq!(DiceRoll::parse("   "), Err(ParseError::Empty));
    }

    #[test]
    fn rejects_unknown_die_sides() {
        assert_eq!(DiceRoll::parse("1d7"), Err(ParseError::UnknownDieSides(7)));
    }

    #[test]
    fn rejects_dangling_d() {
        assert!(matches!(DiceRoll::parse("3d"), Err(ParseError::Malformed { .. })));
    }

    #[test]
    fn rejects_zero_count() {
        assert_eq!(DiceRoll::parse("0d6"), Err(ParseError::ZeroCount));
    }

    #[test]
    fn rejects_trailing_operator() {
        assert!(matches!(DiceRoll::parse("d6+"), Err(ParseError::Malformed { .. })));
    }
}
