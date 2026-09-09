use std::fmt;

use crate::dice::kind::DieKind;
use crate::dice::options::{Options, OptionsError};
use crate::dice::roll::{DiceRoll, DiceTerm};

/// Largest die count a single term may carry (e.g. `1000d6`).
pub const MAX_DICE_PER_TERM: u32 = 1000;

/// Failures returned by [`DiceRoll::parse`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    /// Input was empty or whitespace-only.
    Empty,
    /// The expression contains adjustments without any dice terms.
    NoDice,
    /// A `d` was followed by a number that doesn't match a supported [`DieKind`].
    UnknownDieSides(u32),
    /// A term used a count of zero (e.g. `0d6`).
    ZeroCount,
    /// A term's die count exceeded [`MAX_DICE_PER_TERM`].
    CountTooLarge(u32),
    /// A literal exceeded `u32` or an adjustment exceeded `-i32::MAX..=i32::MAX`.
    Overflow,
    /// A modifier cannot be applied to the term's die kind.
    InvalidOptions(OptionsError),
    /// Generic syntax error at `position` (byte offset) with a short reason.
    Malformed { position: usize, reason: &'static str },
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(formatter, "empty dice expression"),
            ParseError::NoDice => write!(formatter, "expression must contain at least one dice term"),
            ParseError::UnknownDieSides(sides) => write!(formatter, "no die has {sides} sides"),
            ParseError::ZeroCount => write!(formatter, "die count must be at least 1"),
            ParseError::CountTooLarge(count) => {
                write!(formatter, "die count {count} exceeds the limit of {MAX_DICE_PER_TERM}")
            }
            ParseError::Overflow => write!(formatter, "numeric literal or accumulated adjustment out of range"),
            ParseError::InvalidOptions(OptionsError::KeepZero) => write!(formatter, "keep count must be at least 1"),
            ParseError::InvalidOptions(OptionsError::RerollNotBelowMax { reroll, sides }) => {
                write!(formatter, "reroll threshold {reroll} must be below {sides}")
            }
            ParseError::InvalidOptions(OptionsError::ExplodeNotAboveMin { explode }) => {
                write!(formatter, "explode threshold {explode} must be above 1")
            }
            ParseError::Malformed { position, reason } => {
                write!(formatter, "malformed expression at byte {position}: {reason}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl DiceRoll {
    /// Parses case-insensitive `NdK` terms with `khN`, `klN`, `rN`, `eN` suffixes and signed adjustments.
    /// Requires a dice term, rejects leading signs, and limits accumulated adjustments to `-i32::MAX..=i32::MAX`.
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
                    if count > MAX_DICE_PER_TERM {
                        return Err(ParseError::CountTooLarge(count));
                    }
                    let options = lexer.read_options()?;
                    options.validate_for(kind).map_err(ParseError::InvalidOptions)?;
                    terms.push(DiceTerm { count, kind, negate, options });
                }
                Token::Number(value) => {
                    let magnitude = i32::try_from(value).map_err(|_| ParseError::Overflow)?;
                    let signed = if negate { -magnitude } else { magnitude };
                    adjustment = adjustment.checked_add(signed).ok_or(ParseError::Overflow)?;
                    if adjustment == i32::MIN {
                        return Err(ParseError::Overflow);
                    }
                }
            }

            lexer.skip_whitespace();
            if lexer.is_done() {
                break;
            }
            negate = lexer.read_sign()?;
        }

        if terms.is_empty() {
            return Err(ParseError::NoDice);
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
                    .and_then(|partial| partial.checked_add((byte - b'0') as u32))
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
            let sides = self
                .read_digits()?
                .ok_or(ParseError::Malformed { position: self.cursor, reason: "expected digits after 'd'" })?;
            Ok(Token::Dice { count: leading.unwrap_or(1), sides })
        } else {
            let value = leading.ok_or(ParseError::Malformed { position, reason: "expected number or dice term" })?;
            Ok(Token::Number(value))
        }
    }

    fn read_options(&mut self) -> Result<Options, ParseError> {
        let mut options = Options::default();
        loop {
            self.skip_whitespace();
            let position = self.cursor;
            let destination = match self.peek().map(|byte| byte.to_ascii_lowercase()) {
                Some(b'k') => {
                    self.cursor += 1;
                    match self.bump().map(|byte| byte.to_ascii_lowercase()) {
                        Some(b'h') => &mut options.keep_highest,
                        Some(b'l') => &mut options.keep_lowest,
                        _ => return Err(ParseError::Malformed { position, reason: "expected 'kh' or 'kl'" }),
                    }
                }
                Some(b'r') => {
                    self.cursor += 1;
                    &mut options.reroll_at_or_below
                }
                Some(b'e') => {
                    self.cursor += 1;
                    &mut options.explode_at_or_above
                }
                _ => break,
            };
            if destination.is_some() {
                return Err(ParseError::Malformed { position, reason: "duplicate modifier" });
            }
            *destination =
                Some(self.read_digits()?.ok_or(ParseError::Malformed {
                    position: self.cursor,
                    reason: "expected digits after modifier",
                })?);
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};

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
    fn rejects_adjustment_outside_i32() {
        assert_eq!(DiceRoll::parse("d6+2147483648"), Err(ParseError::Overflow));
        assert_eq!(DiceRoll::parse("d6-2147483648"), Err(ParseError::Overflow));
        assert_eq!(DiceRoll::parse("d6+4294967295"), Err(ParseError::Overflow));
        assert_eq!(DiceRoll::parse("d6+2147483647").unwrap().adjustment, i32::MAX);
        assert_eq!(DiceRoll::parse("d6-2147483647").unwrap().adjustment, -i32::MAX);
        for expression in ["1d6-2147483647-1", "1d6-2147483647-1+1", "1d6+2147483647+1", "1d6+4294967296"] {
            assert_eq!(DiceRoll::parse(expression), Err(ParseError::Overflow), "{expression}");
        }
    }

    #[test]
    fn rejects_count_over_limit() {
        assert_eq!(DiceRoll::parse("1001d6"), Err(ParseError::CountTooLarge(1001)));
        assert_eq!(DiceRoll::parse("4294967295d6"), Err(ParseError::CountTooLarge(u32::MAX)));
        assert_eq!(DiceRoll::parse("1000d6").unwrap().terms[0].count, 1000);
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

    #[test]
    fn rejects_adjustments_without_dice() {
        for expression in ["5", "0", "2+3", "5-6", "2147483647"] {
            assert_eq!(DiceRoll::parse(expression), Err(ParseError::NoDice), "{expression}");
        }
        assert_eq!(DiceRoll::parse("5+1d6").unwrap().adjustment, 5);
    }

    #[test]
    fn parses_advantage_and_disadvantage() {
        for roll in [DiceRoll::advantage(5), DiceRoll::disadvantage(-3)] {
            assert_eq!(DiceRoll::parse(&roll.to_string()), Ok(roll));
        }
    }

    #[test]
    fn parsed_advantage_keeps_the_highest_rolled_value() {
        let roll = DiceRoll::parse("2d20kh1+5").unwrap();
        for seed in 0..64 {
            let mut expected_random = StdRng::seed_from_u64(seed);
            let first_value = DieKind::D20.roll(&mut expected_random);
            let second_value = DieKind::D20.roll(&mut expected_random);
            let expected_value = first_value.max(second_value);
            let outcome = roll.roll_detailed(&mut StdRng::seed_from_u64(seed));
            assert_eq!(outcome.total, expected_value as i32 + 5);
            assert_eq!(outcome.term_dice(0).len(), 1);
            assert_eq!(outcome.term_dice(0)[0].value, expected_value);
        }
    }

    #[test]
    fn parses_combined_modifiers_in_any_order_and_case() {
        let options = Options::default()
            .with_keep_highest(3)
            .with_keep_lowest(2)
            .with_reroll_at_or_below(1)
            .with_explode_at_or_above(6);
        for expression in ["4d6kh3kl2r1e6+5", "4D6E6R1KL2KH3+5", " 4D6 r1 KH3 e6 kl2 + 5 "] {
            let roll = DiceRoll::parse(expression).unwrap();
            assert_eq!(roll.terms[0].options, options);
            assert_eq!(roll.adjustment, 5);
        }
        let roll = DiceRoll::parse("2d20kh1-4d6r1e6kl2+3").unwrap();
        assert_eq!(roll.terms[0].options.keep_highest, Some(1));
        assert!(roll.terms[1].negate);
        assert_eq!(roll.terms[1].options.keep_lowest, Some(2));
    }

    #[test]
    fn modifier_combinations_round_trip() {
        for kind in DieKind::ALL {
            for modifier_mask in 0..16 {
                let options = Options {
                    keep_highest: (modifier_mask & 1 != 0).then_some(3),
                    keep_lowest: (modifier_mask & 2 != 0).then_some(2),
                    reroll_at_or_below: (modifier_mask & 4 != 0).then_some(1),
                    explode_at_or_above: (modifier_mask & 8 != 0).then_some(kind.sides()),
                };
                for adjustment in [-i32::MAX, -5, 0, 5, i32::MAX] {
                    let roll = DiceRoll {
                        terms: vec![
                            DiceTerm { count: 4, kind, negate: false, options: options.clone() },
                            DiceTerm { count: 2, kind, negate: true, options: options.clone() },
                        ],
                        adjustment,
                    };
                    let expression = roll.to_string();
                    assert_eq!(DiceRoll::parse(&expression), Ok(roll), "{expression}");
                }
            }
        }
    }

    #[test]
    fn rejects_malformed_modifiers() {
        for expression in [
            "4d6kh",
            "4d6kl",
            "4d6r",
            "4d6e",
            "4d6k3",
            "4d6kh-1",
            "4d6kh1KH2",
            "4d6kl1kl2",
            "4d6r1R2",
            "4d6e6e5",
            "4d6r+1",
            "4d6kh1unknown",
            "5kh1+1d6",
        ] {
            assert!(matches!(DiceRoll::parse(expression), Err(ParseError::Malformed { .. })), "{expression}");
        }
        assert_eq!(DiceRoll::parse("4d6kh4294967296"), Err(ParseError::Overflow));
    }

    #[test]
    fn rejects_invalid_modifier_settings() {
        for expression in ["4d6kh0", "4d6kl0"] {
            assert_eq!(DiceRoll::parse(expression), Err(ParseError::InvalidOptions(OptionsError::KeepZero)));
        }
        assert_eq!(
            DiceRoll::parse("4d6r6"),
            Err(ParseError::InvalidOptions(OptionsError::RerollNotBelowMax { reroll: 6, sides: 6 }))
        );
        for explode in [0, 1] {
            assert_eq!(
                DiceRoll::parse(&format!("4d6e{explode}")),
                Err(ParseError::InvalidOptions(OptionsError::ExplodeNotAboveMin { explode }))
            );
        }
    }
}
