//! Rechenansatz for a quantity: „3*12,5+4“ or „=2*(1,5+0,75)“ – the four
//! basic operations, brackets, German decimal comma. The result is rounded
//! to three decimals (GAEB quantities); the text stays at the position.

use rust_decimal::{Decimal, RoundingStrategy};

/// Whether the input is a calculation and not just a number: it starts with
/// „=“ or holds an operator or bracket after the first character.
pub fn is_formula(text: &str) -> bool {
    let text = text.trim();
    if text.starts_with('=') {
        return true;
    }
    text.char_indices().any(|(index, character)| {
        matches!(character, '+' | '*' | '/' | '(' | ')' | '×' | ':')
            || (index > 0 && character == '-')
    })
}

/// Evaluates a Rechenansatz; the error says what is wrong in German.
pub fn evaluate(text: &str) -> Result<Decimal, String> {
    let source = text.trim().trim_start_matches('=');
    let tokens = tokenize(source)?;
    if tokens.is_empty() {
        return Err("Der Rechenansatz ist leer.".to_string());
    }
    let mut parser = Parser { tokens, at: 0 };
    let value = parser.expression()?;
    match parser.tokens.get(parser.at) {
        None => Ok(value
            .round_dp_with_strategy(3, RoundingStrategy::MidpointAwayFromZero)
            .normalize()),
        Some(Token::Close) => Err("Zu viele schließende Klammern.".to_string()),
        Some(_) => Err("Zwischen zwei Zahlen fehlt ein Rechenzeichen.".to_string()),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(Decimal),
    Plus,
    Minus,
    Times,
    Divide,
    Open,
    Close,
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let characters: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        let token = match character {
            ' ' | '\t' => {
                index += 1;
                continue;
            }
            '+' => Token::Plus,
            '-' | '−' => Token::Minus,
            '*' | '×' | 'x' | 'X' => Token::Times,
            '/' | ':' | '÷' => Token::Divide,
            '(' => Token::Open,
            ')' => Token::Close,
            '0'..='9' | ',' | '.' => {
                let start = index;
                while index < characters.len() && matches!(characters[index], '0'..='9' | ',' | '.')
                {
                    index += 1;
                }
                let piece: String = characters[start..index].iter().collect();
                tokens.push(Token::Number(number(&piece)?));
                continue;
            }
            other => return Err(format!("„{other}“ gehört nicht in einen Rechenansatz.")),
        };
        tokens.push(token);
        index += 1;
    }
    Ok(tokens)
}

/// „12,5“, „1.234,5“, „1.150“ (= 1150, one dot with three digits after it
/// is a thousands dot), „2.5“ (= 2,5).
fn number(piece: &str) -> Result<Decimal, String> {
    let wrong = || format!("„{piece}“ ist keine Zahl.");
    let normalized = if piece.contains(',') {
        if piece.matches(',').count() > 1 {
            return Err(wrong());
        }
        piece.replace('.', "").replace(',', ".")
    } else if piece.matches('.').count() > 1 {
        piece.replace('.', "")
    } else if let Some((before, after)) = piece.split_once('.') {
        if after.len() == 3 && !before.is_empty() && before.len() <= 3 && !before.starts_with('0') {
            format!("{before}{after}")
        } else {
            piece.to_string()
        }
    } else {
        piece.to_string()
    };
    if normalized.starts_with('.') || normalized.ends_with('.') {
        return Err(wrong());
    }
    normalized.parse::<Decimal>().map_err(|_| wrong())
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn expression(&mut self) -> Result<Decimal, String> {
        let mut value = self.term()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => {
                    self.at += 1;
                    value = value.checked_add(self.term()?).ok_or_else(too_big)?;
                }
                Some(Token::Minus) => {
                    self.at += 1;
                    value = value.checked_sub(self.term()?).ok_or_else(too_big)?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn term(&mut self) -> Result<Decimal, String> {
        let mut value = self.factor()?;
        loop {
            match self.peek() {
                Some(Token::Times) => {
                    self.at += 1;
                    value = value.checked_mul(self.factor()?).ok_or_else(too_big)?;
                }
                Some(Token::Divide) => {
                    self.at += 1;
                    let divisor = self.factor()?;
                    if divisor.is_zero() {
                        return Err("Division durch 0.".to_string());
                    }
                    value = value.checked_div(divisor).ok_or_else(too_big)?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn factor(&mut self) -> Result<Decimal, String> {
        match self.tokens.get(self.at).cloned() {
            Some(Token::Number(value)) => {
                self.at += 1;
                Ok(value)
            }
            Some(Token::Minus) => {
                self.at += 1;
                Ok(-self.factor()?)
            }
            Some(Token::Plus) => {
                self.at += 1;
                self.factor()
            }
            Some(Token::Open) => {
                self.at += 1;
                let value = self.expression()?;
                match self.peek() {
                    Some(Token::Close) => {
                        self.at += 1;
                        Ok(value)
                    }
                    _ => Err("Klammer fehlt: zu einer „(“ fehlt die „)“.".to_string()),
                }
            }
            Some(Token::Close) => Err("Vor „)“ fehlt eine Zahl.".to_string()),
            Some(_) => {
                Err("Zwei Rechenzeichen hintereinander – dazwischen fehlt eine Zahl.".to_string())
            }
            None => Err("Am Ende fehlt eine Zahl.".to_string()),
        }
    }
}

fn too_big() -> String {
    "Das Ergebnis ist zu groß.".to_string()
}

#[cfg(test)]
mod tests {
    use super::{evaluate, is_formula};
    use rust_decimal::Decimal;

    fn value(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    #[test]
    fn calculates_with_german_numbers() {
        assert_eq!(evaluate("3*12,5+4").unwrap(), value("41.5"));
        assert_eq!(evaluate("=2*(1,5+0,75)").unwrap(), value("4.5"));
        assert_eq!(evaluate("10-2-3").unwrap(), value("5"));
        assert_eq!(evaluate("2+3*4").unwrap(), value("14"));
        assert_eq!(evaluate("(2+3)*4").unwrap(), value("20"));
        assert_eq!(evaluate("12/8").unwrap(), value("1.5"));
        assert_eq!(evaluate("10/3").unwrap(), value("3.333"));
        assert_eq!(evaluate("2/3").unwrap(), value("0.667"));
        assert_eq!(evaluate("-2*-3").unwrap(), value("6"));
        assert_eq!(evaluate(" 4 x 2,5 ").unwrap(), value("10"));
        assert_eq!(evaluate("1.150+1").unwrap(), value("1151"));
        assert_eq!(evaluate("1.234,5*2").unwrap(), value("2469"));
        assert_eq!(evaluate("2.5*2").unwrap(), value("5"));
        assert_eq!(evaluate("=((1))").unwrap(), value("1"));
    }

    #[test]
    fn errors_say_what_is_wrong() {
        assert!(evaluate("2*(3+4").unwrap_err().contains("Klammer fehlt"));
        assert!(evaluate("2*3)").unwrap_err().contains("schließende"));
        assert!(evaluate("5/0").unwrap_err().contains("Division durch 0"));
        assert!(evaluate("5/(2-2)")
            .unwrap_err()
            .contains("Division durch 0"));
        assert!(evaluate("2**3").unwrap_err().contains("Rechenzeichen"));
        assert!(evaluate("2+").unwrap_err().contains("Am Ende"));
        assert!(evaluate("=").unwrap_err().contains("leer"));
        assert!(evaluate("2 3").unwrap_err().contains("Rechenzeichen"));
        assert!(evaluate("2,5,1").unwrap_err().contains("keine Zahl"));
        assert!(evaluate("2a").unwrap_err().contains("„a“"));
        assert!(evaluate("()").unwrap_err().contains("„)“"));
    }

    #[test]
    fn recognises_formulas() {
        assert!(is_formula("=5"));
        assert!(is_formula("3*4"));
        assert!(is_formula("2-1"));
        assert!(!is_formula("-5"));
        assert!(!is_formula("12,5"));
        assert!(!is_formula("1.234,5"));
    }
}
