pub mod execution;
pub mod grammar;
pub mod parser;
pub mod semantic;
pub mod syntax;
pub use execution::*;
pub use grammar::*;
pub use parser::{ParseOptions, TemplateHoles, parse, parse_template, parse_with_options};
pub use semantic::*;
pub use syntax::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub category: &'static str,
    pub span: [u32; 2],
    pub message: String,
    /// The catalog code of a failed source check.
    pub code: Option<&'static str>,
}

impl Diagnostic {
    pub fn new(category: &'static str, span: [u32; 2], message: impl Into<String>) -> Self {
        Self {
            category,
            span,
            message: message.into(),
            code: None,
        }
    }
}

/// The day number (days since 0001-01-01, proleptic Gregorian) of an exact
/// `yyyy-MM-dd` spelling; `None` when it is not a calendar date.
pub fn iso_date_day_number(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year < 1 || !(1..=12).contains(&month) || day < 1 || day > lengths[month as usize - 1] {
        return None;
    }
    let before = year - 1;
    let days_before_year = before * 365 + before / 4 - before / 100 + before / 400;
    let days_before_month: i64 = lengths[..month as usize - 1].iter().sum();
    Some(days_before_year + days_before_month + day - 1)
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Semantic nesting: HIR depth and rewrite expansion depth.
    pub depth: u32,
    pub work: u64,
    /// Parser recursion frames. Generated wrappers consume frames too, so a
    /// layered grammar such as C# needs several frames per nesting level.
    pub parser_frames: u32,
    /// The parser may also spend this much work per input byte, so large
    /// files parse while superlinear backtracking still stops.
    pub parser_work_per_byte: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            depth: 128,
            work: 10_000_000,
            parser_frames: 512,
            parser_work_per_byte: 4096,
        }
    }
}

mod templates;
