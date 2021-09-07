use once_cell::sync::Lazy;
use regex::Regex;

use super::Span;

static IDENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]?$").unwrap());

#[derive(Clone, Copy, Debug)]
pub struct ReportedError {
    pub kind: ErrorKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug)]
pub enum ErrorKind {
    InvalidIdent,
    MalformedHeader,
    SpaceAfterMultiline,
    SlashSlashComent,
    BadValue,
    Other,
}

#[derive(Clone, Debug)]
pub enum Diag {
    Ok,
    None,
    Err(Vec<ReportedError>),
}

pub fn try_report_comment(text: &str, span: &Span) -> Diag {
    let line = &text[span];
    let trimmed_line = line.trim();

    if trimmed_line.starts_with(';') {
        Diag::Ok
    } else if trimmed_line.starts_with(r"//") {
        Diag::Err(vec![ReportedError {
            span: *span,
            kind: ErrorKind::SlashSlashComent,
        }])
    } else {
        Diag::None
    }
}

pub fn try_report_section_error(text: &str, span: &Span) -> Diag {
    let line = &text[span];
    let trimmed_line = if let Some(pos) = line.find(';') {
        line[..pos].trim()
    } else {
        line.trim()
    };

    if matches!(
        (
            trimmed_line.as_bytes().first(),
            trimmed_line.as_bytes().last()
        ),
        (Some(b'['), Some(b']'))
    ) {
        Diag::Err(vec![ReportedError {
            span: *span,
            kind: ErrorKind::MalformedHeader,
        }])
    } else {
        Diag::None
    }
}

pub fn validate_property_text(text: &str, span: &Span, strict: bool) -> Diag {
    // And this is where this whole thing becomes a bit sad.
    // Basically any property text is valid because the UE3
    // config parser doesn't care about types -- it's strings
    // all the way down. In fact, even the regex used for keys
    // already excludes things the config parser happily accepts.
    //
    // As a result, this function needs to be a bit creative with guessing
    // what the user intended in order to not yield too many false positives.

    if text[span].is_empty() {
        return Diag::Ok;
    }

    // First, clear out the backslashes and direct newlines
    let mut reduced = String::new();
    let mut start = span.0;
    loop {
        match text[start..span.1].find(|c| matches!(c, '\t' | ' ')) {
            Some(eol) if text.get(start + eol - 2..start + eol) == Some(r"\\") => {
                reduced.push_str(&text[start..start + eol - 2]);
                reduced.push_str("  ");
                start += eol;

                while matches!(text.as_bytes().get(start), Some(b'\t' | b'\r' | b'\n')) {
                    start += 1;
                    reduced.push(' ');
                }
            }
            Some(_) | None => {
                reduced.push_str(&text[start..span.1]);
                break;
            }
        }
    }

    // Then, unescape if needed
    if reduced.as_bytes().first() == Some(&b'"') {
        let mut span = *span;
        span.0 += 1;
        if reduced.as_bytes().last() == Some(&b'"') {
            span.1 -= 1;
        }
        reduced = reduced[1..reduced.len() - 1].replace("\\\\", "\\");
        reduced = reduced.replace("\\\"", "\"");
        reduced = reduced.replace("\\n", "\n");

        Diag::Ok
    } else {
        if matches_bool(&reduced) {
            return Diag::Ok;
        }

        if matches_num(&reduced) {
            return Diag::Ok;
        }

        if matches_ident(&reduced) {
            return Diag::Ok;
        }

        Diag::Err(vec![ReportedError {
            kind: ErrorKind::BadValue,
            span: *span,
        }])
    }
}

fn matches_bool(text: &str) -> bool {
    matches!(&*text.to_ascii_lowercase(), "true" | "false")
}

fn matches_num(text: &str) -> bool {
    text.parse::<i32>().is_ok() || text.parse::<f32>().is_ok()
}

fn matches_ident(text: &str) -> bool {
    IDENT.is_match(text)
}
