use std::fmt;

/// Dimension tracked during CSS `calc()` / `min()` / `max()` / `clamp()` evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathDim {
    Number,
    Px,
    Percent,
    Deg,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalcVal {
    pub value: f32,
    pub dim: MathDim,
}

struct MathParser<'a> {
    src: &'a [u8],
    pos: usize,
    font_size: f32,
    root_font_size: f32,
    vw: f32,
    vh: f32,
    percent_basis: Option<f32>,
}

impl<'a> MathParser<'a> {
    fn new(
        s: &'a str,
        font_size: f32,
        root_font_size: f32,
        vw: f32,
        vh: f32,
        percent_basis: Option<f32>,
    ) -> Self {
        Self {
            src: s.as_bytes(),
            pos: 0,
            font_size,
            root_font_size,
            vw,
            vh,
            percent_basis,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.src.get(self.pos).copied()
    }

    fn parse_expr(&mut self) -> Option<CalcVal> {
        let mut lhs = self.parse_term()?;
        loop {
            self.skip_ws();
            if self.pos >= self.src.len() {
                break;
            }
            let op = self.src[self.pos];
            if op != b'+' && op != b'-' {
                break;
            }
            // Enforce CSS spec whitespace or valid token boundary around + / -
            self.pos += 1;
            let rhs = self.parse_term()?;
            lhs = combine_add_sub(lhs, rhs, op == b'+', self.percent_basis)?;
        }
        Some(lhs)
    }

    fn parse_term(&mut self) -> Option<CalcVal> {
        let mut lhs = self.parse_unary()?;
        loop {
            self.skip_ws();
            if self.pos >= self.src.len() {
                break;
            }
            let op = self.src[self.pos];
            if op != b'*' && op != b'/' {
                break;
            }
            self.pos += 1;
            let rhs = self.parse_unary()?;
            lhs = if op == b'*' {
                match (lhs.dim, rhs.dim) {
                    (MathDim::Number, dim) => CalcVal {
                        value: lhs.value * rhs.value,
                        dim,
                    },
                    (dim, MathDim::Number) => CalcVal {
                        value: lhs.value * rhs.value,
                        dim,
                    },
                    _ => return None,
                }
            } else {
                if rhs.dim != MathDim::Number || rhs.value.abs() < 1e-12 {
                    return None;
                }
                CalcVal {
                    value: lhs.value / rhs.value,
                    dim: lhs.dim,
                }
            };
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<CalcVal> {
        self.skip_ws();
        if self.pos < self.src.len() {
            if self.src[self.pos] == b'+' {
                self.pos += 1;
                return self.parse_primary();
            }
            if self.src[self.pos] == b'-' {
                self.pos += 1;
                let mut v = self.parse_primary()?;
                v.value = -v.value;
                return Some(v);
            }
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<CalcVal> {
        self.skip_ws();
        if self.pos >= self.src.len() {
            return None;
        }
        if self.src[self.pos] == b'(' {
            self.pos += 1;
            let val = self.parse_expr()?;
            if self.peek() != Some(b')') {
                return None;
            }
            self.pos += 1;
            return Some(val);
        }

        // Named constants or nested math functions (`calc`, `min`, `max`, `clamp`, `pi`, `e`)
        if self.src[self.pos].is_ascii_alphabetic() {
            let start = self.pos;
            while self.pos < self.src.len()
                && (self.src[self.pos].is_ascii_alphanumeric() || self.src[self.pos] == b'-')
            {
                self.pos += 1;
            }
            let ident = std::str::from_utf8(&self.src[start..self.pos]).ok()?;
            self.skip_ws();
            if self.pos < self.src.len() && self.src[self.pos] == b'(' {
                self.pos += 1;
                let res = self.parse_func_args(ident)?;
                if self.peek() != Some(b')') {
                    return None;
                }
                self.pos += 1;
                return Some(res);
            }
            return match ident.to_ascii_lowercase().as_str() {
                "pi" => Some(CalcVal {
                    value: std::f32::consts::PI,
                    dim: MathDim::Number,
                }),
                "e" => Some(CalcVal {
                    value: std::f32::consts::E,
                    dim: MathDim::Number,
                }),
                _ => None,
            };
        }

        // Numeric literal + optional CSS unit
        let num_start = self.pos;
        if matches!(self.src[self.pos], b'+' | b'-') {
            self.pos += 1;
        }
        let mut saw_digit = false;
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
            saw_digit = true;
            self.pos += 1;
        }
        if self.pos < self.src.len() && self.src[self.pos] == b'.' {
            self.pos += 1;
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                saw_digit = true;
                self.pos += 1;
            }
        }
        if !saw_digit {
            return None;
        }
        if self.pos < self.src.len() && matches!(self.src[self.pos], b'e' | b'E') {
            let save = self.pos;
            self.pos += 1;
            if self.pos < self.src.len() && matches!(self.src[self.pos], b'+' | b'-') {
                self.pos += 1;
            }
            let mut exp_digits = false;
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                exp_digits = true;
                self.pos += 1;
            }
            if !exp_digits {
                self.pos = save;
            }
        }

        let num_str = std::str::from_utf8(&self.src[num_start..self.pos]).ok()?;
        let num: f32 = num_str.parse().ok()?;

        // Unit suffix
        if self.pos < self.src.len() && self.src[self.pos] == b'%' {
            self.pos += 1;
            return Some(if let Some(basis) = self.percent_basis {
                CalcVal {
                    value: basis * (num * 0.01),
                    dim: MathDim::Px,
                }
            } else {
                CalcVal {
                    value: num,
                    dim: MathDim::Percent,
                }
            });
        }

        let unit_start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_alphabetic() {
            self.pos += 1;
        }
        let unit = std::str::from_utf8(&self.src[unit_start..self.pos])
            .ok()?
            .to_ascii_lowercase();
        match unit.as_str() {
            "" => Some(CalcVal {
                value: num,
                dim: MathDim::Number,
            }),
            "px" => Some(CalcVal {
                value: num,
                dim: MathDim::Px,
            }),
            "rem" => Some(CalcVal {
                value: num * self.root_font_size,
                dim: MathDim::Px,
            }),
            "em" => Some(CalcVal {
                value: num * self.font_size,
                dim: MathDim::Px,
            }),
            "vw" => Some(CalcVal {
                value: num * 0.01 * self.vw,
                dim: MathDim::Px,
            }),
            "vh" => Some(CalcVal {
                value: num * 0.01 * self.vh,
                dim: MathDim::Px,
            }),
            "vmin" => Some(CalcVal {
                value: num * 0.01 * self.vw.min(self.vh),
                dim: MathDim::Px,
            }),
            "vmax" => Some(CalcVal {
                value: num * 0.01 * self.vw.max(self.vh),
                dim: MathDim::Px,
            }),
            "deg" => Some(CalcVal {
                value: num,
                dim: MathDim::Deg,
            }),
            "turn" => Some(CalcVal {
                value: num * 360.0,
                dim: MathDim::Deg,
            }),
            "rad" => Some(CalcVal {
                value: num.to_degrees(),
                dim: MathDim::Deg,
            }),
            "grad" => Some(CalcVal {
                value: num * 0.9,
                dim: MathDim::Deg,
            }),
            _ => None,
        }
    }

    fn parse_func_args(&mut self, name: &str) -> Option<CalcVal> {
        let mut args = Vec::new();
        args.push(self.parse_expr()?);
        while self.peek() == Some(b',') {
            self.pos += 1;
            args.push(self.parse_expr()?);
        }

        if name.eq_ignore_ascii_case("calc") && args.len() == 1 {
            return Some(args[0]);
        }
        if name.eq_ignore_ascii_case("min") && !args.is_empty() {
            return fold_extremum(&args, true, self.percent_basis);
        }
        if name.eq_ignore_ascii_case("max") && !args.is_empty() {
            return fold_extremum(&args, false, self.percent_basis);
        }
        if name.eq_ignore_ascii_case("clamp") && args.len() == 3 {
            let min_v = args[0];
            let mid_v = args[1];
            let max_v = args[2];
            let (d, a, b, c) = unify_3(min_v, mid_v, max_v, self.percent_basis)?;
            // CSS clamp(MIN, VAL, MAX) = max(MIN, min(VAL, MAX))
            let val = a.max(b.min(c));
            return Some(CalcVal { value: val, dim: d });
        }
        if name.eq_ignore_ascii_case("abs") && args.len() == 1 {
            return Some(CalcVal {
                value: args[0].value.abs(),
                dim: args[0].dim,
            });
        }
        None
    }
}

fn unify_pair(a: CalcVal, b: CalcVal, percent_basis: Option<f32>) -> Option<(MathDim, f32, f32)> {
    let to_px = |v: CalcVal| -> Option<CalcVal> {
        if v.dim == MathDim::Percent {
            percent_basis.map(|basis| CalcVal {
                value: basis * (v.value * 0.01),
                dim: MathDim::Px,
            })
        } else {
            Some(v)
        }
    };
    let a = to_px(a)?;
    let b = to_px(b)?;
    match (a.dim, b.dim) {
        (d1, d2) if d1 == d2 => Some((d1, a.value, b.value)),
        (MathDim::Px, MathDim::Number) if b.value == 0.0 => Some((MathDim::Px, a.value, 0.0)),
        (MathDim::Number, MathDim::Px) if a.value == 0.0 => Some((MathDim::Px, 0.0, b.value)),
        _ => None,
    }
}

fn unify_3(
    a: CalcVal,
    b: CalcVal,
    c: CalcVal,
    percent_basis: Option<f32>,
) -> Option<(MathDim, f32, f32, f32)> {
    let (d1, av, bv) = unify_pair(a, b, percent_basis)?;
    let (d2, _, cv) = unify_pair(CalcVal { value: av, dim: d1 }, c, percent_basis)?;
    Some((d2, av, bv, cv))
}

fn combine_add_sub(
    lhs: CalcVal,
    rhs: CalcVal,
    add: bool,
    percent_basis: Option<f32>,
) -> Option<CalcVal> {
    let (dim, a, b) = unify_pair(lhs, rhs, percent_basis)?;
    Some(CalcVal {
        value: if add { a + b } else { a - b },
        dim,
    })
}

fn fold_extremum(args: &[CalcVal], is_min: bool, percent_basis: Option<f32>) -> Option<CalcVal> {
    let mut acc = args[0];
    for &item in &args[1..] {
        let (dim, a, b) = unify_pair(acc, item, percent_basis)?;
        acc = CalcVal {
            value: if is_min { a.min(b) } else { a.max(b) },
            dim,
        };
    }
    Some(acc)
}

/// Evaluates a standalone `calc(...)`, `min(...)`, `max(...)`, or `clamp(...)` expression.
pub fn eval_css_math(
    expr: &str,
    font_size: f32,
    root_font_size: f32,
    vw: f32,
    vh: f32,
    percent_basis: Option<f32>,
) -> Option<CalcVal> {
    let trimmed = expr.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("calc(")
        || lower.starts_with("min(")
        || lower.starts_with("max(")
        || lower.starts_with("clamp(")
        || lower.starts_with("abs("))
    {
        return None;
    }
    let mut parser = MathParser::new(trimmed, font_size, root_font_size, vw, vh, percent_basis);
    let val = parser.parse_expr()?;
    parser.skip_ws();
    if parser.pos != parser.src.len() {
        return None;
    }
    Some(val)
}

fn fmt_val(v: CalcVal) -> String {
    let rounded = if (v.value - v.value.round()).abs() < 1e-4 {
        v.value.round()
    } else {
        (v.value * 10000.0).round() / 10000.0
    };
    let mut num = fmt_num(rounded);
    match v.dim {
        MathDim::Px => num.push_str("px"),
        MathDim::Percent => num.push('%'),
        MathDim::Deg => num.push_str("deg"),
        MathDim::Number => {}
    }
    num
}

fn fmt_num(v: f32) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

struct FnSpan {
    start: usize,
    end: usize,
}

fn find_next_math_call(s: &str, from: usize) -> Option<FnSpan> {
    let lower = s.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut idx = from;
    while idx < bytes.len() {
        let matched = ["calc(", "min(", "max(", "clamp("]
            .iter()
            .find(|kw| {
                lower[idx..].starts_with(*kw)
                    && (idx == 0
                        || !(bytes[idx - 1].is_ascii_alphanumeric()
                            || bytes[idx - 1] == b'-'
                            || bytes[idx - 1] == b'_'))
            })
            .copied();
        if let Some(kw) = matched {
            let open = idx + kw.len() - 1;
            let mut depth = 0usize;
            let mut in_quote: Option<u8> = None;
            let raw_bytes = s.as_bytes();
            let mut j = open;
            while j < raw_bytes.len() {
                let b = raw_bytes[j];
                if let Some(q) = in_quote {
                    if b == q {
                        in_quote = None;
                    }
                    j += 1;
                    continue;
                }
                match b {
                    b'"' | b'\'' => in_quote = Some(b),
                    b'(' => depth += 1,
                    b')' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return Some(FnSpan {
                                start: idx,
                                end: j + 1,
                            });
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            return None;
        }
        idx += 1;
    }
    None
}

/// Replaces resolvable `calc()`/`min()`/`max()`/`clamp()` calls inside any CSS value string.
pub fn simplify_math_in_value(
    raw: &str,
    font_size: f32,
    root_font_size: f32,
    vw: f32,
    vh: f32,
) -> String {
    let lower = raw.to_ascii_lowercase();
    if !(lower.contains("calc(")
        || lower.contains("min(")
        || lower.contains("max(")
        || lower.contains("clamp("))
    {
        return raw.to_string();
    }

    let mut out = String::with_capacity(raw.len());
    let mut cursor = 0usize;
    while let Some(span) = find_next_math_call(raw, cursor) {
        out.push_str(&raw[cursor..span.start]);
        let call = &raw[span.start..span.end];
        if let Some(val) = eval_css_math(call, font_size, root_font_size, vw, vh, None) {
            out.push_str(&fmt_val(val));
        } else {
            out.push_str(call);
        }
        cursor = span.end;
    }
    out.push_str(&raw[cursor..]);
    out
}

impl fmt::Display for CalcVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&fmt_val(*self))
    }
}
