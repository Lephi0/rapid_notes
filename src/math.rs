use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct LineMathResult {
    pub line_idx: usize,
    pub value: f64,
    pub formatted: String,
    pub has_trailing_equals: bool,
}

/// Evaluates Antinote-style inline arithmetic, variables, percentages, and `sum`/`total`/`avg`
/// across lines in a note, skipping Markdown headings and fenced code blocks.
pub fn evaluate_note_math(content: &str) -> Vec<LineMathResult> {
    let mut results = Vec::new();
    let mut vars: HashMap<String, f64> = HashMap::new();
    let mut recent_values: Vec<f64> = Vec::new();
    let mut in_code_fence = false;

    for (line_idx, raw_line) in content.lines().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            recent_values.clear();
            continue;
        }
        if in_code_fence || trimmed.is_empty() {
            recent_values.clear();
            continue;
        }
        if trimmed.starts_with('#')
            || trimmed.starts_with("- [")
            || trimmed.starts_with("* [")
            || trimmed.starts_with('>')
            || is_date_or_version_like(trimmed)
        {
            recent_values.clear();
            continue;
        }

        let has_trailing_equals = trimmed.ends_with('=');
        let expr_slice = if has_trailing_equals {
            trimmed[..trimmed.len() - 1].trim()
        } else {
            trimmed
        };

        if expr_slice.is_empty() {
            continue;
        }

        // Handle assignment `var = expr` (not `expr =`)
        if let Some((lhs, rhs)) = split_assignment(expr_slice) {
            if let Some((val, currency)) = eval_line_expr(rhs, &vars, &recent_values, true) {
                vars.insert(lhs.to_ascii_lowercase(), val);
                recent_values.push(val);
                results.push(LineMathResult {
                    line_idx,
                    value: val,
                    formatted: format_result(val, currency),
                    has_trailing_equals,
                });
                continue;
            }
        }

        // Pure numeric line (no operators, not ending in `=`) feeds `sum`/`total`/`avg` silently
        if !has_trailing_equals && !contains_math_trigger(expr_slice) {
            if let Some((val, _)) = eval_plain_number(expr_slice) {
                recent_values.push(val);
            } else {
                recent_values.clear();
            }
            continue;
        }

        if let Some((val, currency)) =
            eval_line_expr(expr_slice, &vars, &recent_values, has_trailing_equals)
        {
            recent_values.push(val);
            results.push(LineMathResult {
                line_idx,
                value: val,
                formatted: format_result(val, currency),
                has_trailing_equals,
            });
        } else {
            recent_values.clear();
        }
    }

    results
}

fn is_date_or_version_like(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() >= 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && (bytes[4] == b'-' || bytes[4] == b'/')
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == bytes[4]
        && bytes[8..10].iter().all(u8::is_ascii_digit)
        && line.len() == 10
    {
        return true;
    }
    false
}

fn split_assignment(s: &str) -> Option<(&str, &str)> {
    let eq_idx = s.find('=')?;
    let lhs = s[..eq_idx].trim();
    let rhs = s[eq_idx + 1..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    let valid_ident = lhs
        .chars()
        .enumerate()
        .all(|(i, c)| (i == 0 && (c.is_ascii_alphabetic() || c == '_')) || c.is_ascii_alphanumeric() || c == '_');
    if valid_ident {
        Some((lhs, rhs))
    } else {
        None
    }
}

fn contains_math_trigger(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    if matches!(lower.as_str(), "sum" | "total" | "avg" | "mean") {
        return true;
    }
    s.contains('+')
        || s.contains('-')
        || s.contains('*')
        || s.contains('/')
        || s.contains('×')
        || s.contains('÷')
        || s.contains('^')
        || s.contains('%')
        || s.contains('(')
}

fn eval_plain_number(s: &str) -> Option<(f64, Option<char>)> {
    let mut currency = None;
    let mut stripped = String::with_capacity(s.len());
    for ch in s.trim().chars() {
        if matches!(ch, '$' | '€' | '£') {
            currency.get_or_insert(ch);
        } else if ch == ',' {
            continue;
        } else {
            stripped.push(ch);
        }
    }
    let val: f64 = stripped.trim().parse().ok()?;
    val.is_finite().then_some((val, currency))
}

fn eval_line_expr(
    raw: &str,
    vars: &HashMap<String, f64>,
    recent_values: &[f64],
    allow_bare_var: bool,
) -> Option<(f64, Option<char>)> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "sum" | "total" if !recent_values.is_empty() => {
            return Some((recent_values.iter().sum(), None));
        }
        "avg" | "mean" if !recent_values.is_empty() => {
            let sum: f64 = recent_values.iter().sum();
            return Some((sum / recent_values.len() as f64, None));
        }
        _ => {}
    }

    // Handle "X% of Y"
    if let Some((pct_part, target_part)) = split_percent_of(&lower, trimmed) {
        if let (Some((pct, _)), Some((base, cur))) = (
            parse_and_eval(pct_part, vars, recent_values),
            parse_and_eval(target_part, vars, recent_values),
        ) {
            return Some((base * (pct / 100.0), cur));
        }
    }

    if !allow_bare_var && !contains_math_trigger(trimmed) {
        return None;
    }

    parse_and_eval(trimmed, vars, recent_values)
}

fn split_percent_of<'a>(lower: &str, original: &'a str) -> Option<(&'a str, &'a str)> {
    let marker = "% of ";
    let idx = lower.find(marker)?;
    let lhs = original[..idx].trim();
    let rhs = original[idx + marker.len()..].trim();
    if !lhs.is_empty() && !rhs.is_empty() {
        Some((lhs, rhs))
    } else {
        None
    }
}

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Percent(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Option<(Vec<Token>, Option<char>)> {
    let mut tokens = Vec::new();
    let mut currency = None;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if matches!(ch, '$' | '€' | '£') {
            currency.get_or_insert(ch);
            i += 1;
            continue;
        }
        if ch.is_ascii_digit() || (ch == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let mut num_str = String::new();
            let mut dot_seen = false;
            while i < chars.len() {
                let c = chars[i];
                if c.is_ascii_digit() {
                    num_str.push(c);
                    i += 1;
                } else if c == '.' && !dot_seen {
                    dot_seen = true;
                    num_str.push(c);
                    i += 1;
                } else if c == ',' && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                    i += 1;
                } else {
                    break;
                }
            }
            let val: f64 = num_str.parse().ok()?;
            if chars.get(i) == Some(&'%') {
                i += 1;
                tokens.push(Token::Percent(val));
            } else {
                tokens.push(Token::Number(val));
            }
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut ident = String::new();
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                ident.push(chars[i]);
                i += 1;
            }
            tokens.push(Token::Ident(ident.to_ascii_lowercase()));
            continue;
        }
        let tok = match ch {
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' | '×' => Token::Star,
            '/' | '÷' => Token::Slash,
            '^' => Token::Caret,
            '(' => Token::LParen,
            ')' => Token::RParen,
            _ => return None,
        };
        tokens.push(tok);
        i += 1;
    }

    Some((tokens, currency))
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    vars: &'a HashMap<String, f64>,
    recent_values: &'a [f64],
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn parse_expr(&mut self) -> Option<f64> {
        let mut lhs = self.parse_term()?;
        while let Some(tok) = self.peek() {
            match tok {
                Token::Plus => {
                    self.next();
                    if let Some(Token::Percent(pct)) = self.peek().cloned() {
                        self.next();
                        lhs += lhs * (pct / 100.0);
                    } else {
                        let rhs = self.parse_term()?;
                        lhs += rhs;
                    }
                }
                Token::Minus => {
                    self.next();
                    if let Some(Token::Percent(pct)) = self.peek().cloned() {
                        self.next();
                        lhs -= lhs * (pct / 100.0);
                    } else {
                        let rhs = self.parse_term()?;
                        lhs -= rhs;
                    }
                }
                _ => break,
            }
        }
        lhs.is_finite().then_some(lhs)
    }

    fn parse_term(&mut self) -> Option<f64> {
        let mut lhs = self.parse_power()?;
        while let Some(tok) = self.peek() {
            match tok {
                Token::Star => {
                    self.next();
                    let rhs = self.parse_power()?;
                    lhs *= rhs;
                }
                Token::Slash => {
                    self.next();
                    let rhs = self.parse_power()?;
                    if rhs.abs() < f64::EPSILON {
                        return None;
                    }
                    lhs /= rhs;
                }
                _ => break,
            }
        }
        Some(lhs)
    }

    fn parse_power(&mut self) -> Option<f64> {
        let base = self.parse_unary()?;
        if matches!(self.peek(), Some(Token::Caret)) {
            self.next();
            let exp = self.parse_power()?;
            let res = base.powf(exp);
            res.is_finite().then_some(res)
        } else {
            Some(base)
        }
    }

    fn parse_unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Token::Plus) => {
                self.next();
                self.parse_unary()
            }
            Some(Token::Minus) => {
                self.next();
                self.parse_unary().map(|v| -v)
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        let tok = self.next()?.clone();
        match tok {
            Token::Number(n) => Some(n),
            Token::Percent(p) => Some(p / 100.0),
            Token::Ident(name) => match name.as_str() {
                "sum" | "total" if !self.recent_values.is_empty() => {
                    Some(self.recent_values.iter().sum())
                }
                "avg" | "mean" if !self.recent_values.is_empty() => {
                    Some(self.recent_values.iter().sum::<f64>() / self.recent_values.len() as f64)
                }
                "pi" => Some(std::f64::consts::PI),
                other => self.vars.get(other).copied(),
            },
            Token::LParen => {
                let inner = self.parse_expr()?;
                match self.next() {
                    Some(Token::RParen) => Some(inner),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

fn parse_and_eval(
    input: &str,
    vars: &HashMap<String, f64>,
    recent_values: &[f64],
) -> Option<(f64, Option<char>)> {
    let (tokens, currency) = tokenize(input)?;
    if tokens.is_empty() {
        return None;
    }
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        vars,
        recent_values,
    };
    let val = parser.parse_expr()?;
    if parser.pos != tokens.len() || !val.is_finite() {
        return None;
    }
    Some((val, currency))
}

pub fn format_result(value: f64, currency: Option<char>) -> String {
    let rounded = if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        let s = format!("{value:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };

    match currency {
        Some(sym) => {
            if let Some(rest) = rounded.strip_prefix('-') {
                format!("-{sym}{rest}")
            } else {
                format!("{sym}{rounded}")
            }
        }
        None => rounded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_math_arithmetic_vars_percentages_and_sum() {
        let note = "\
# Budget
rent = 1,200
utilities = 150
rent + utilities =
20% of 150
100 + 15%
$49.95 * 2 =
sum
2026-11-21
";
        let res = evaluate_note_math(note);
        let by_line: HashMap<usize, &LineMathResult> =
            res.iter().map(|r| (r.line_idx, r)).collect();

        assert_eq!(by_line[&1].formatted, "1200");
        assert_eq!(by_line[&2].formatted, "150");
        assert_eq!(by_line[&3].formatted, "1350");
        assert!(by_line[&3].has_trailing_equals);
        assert_eq!(by_line[&4].formatted, "30");
        assert_eq!(by_line[&5].formatted, "115");
        assert_eq!(by_line[&6].formatted, "$99.9");
        assert!(by_line[&7].value > 2000.0);
        assert!(!by_line.contains_key(&0));
        assert!(!by_line.contains_key(&8));
    }

    #[test]
    fn test_math_skips_code_fences_and_division_by_zero() {
        let note = "\
```
2 + 2
```
10 / 0
(3 + 5) * 2^2
";
        let res = evaluate_note_math(note);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].line_idx, 4);
        assert_eq!(res[0].formatted, "32");
    }
}
