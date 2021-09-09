use once_cell::sync::Lazy;
use regex::Regex;

use crate::parse::{Directive, Directives, Kvp, KvpOperation, SectionHeader, Span, Unknown};

mod struct_syntax;

static KEY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[A-Za-z][A-Za-z0-9_]*(\[(0|[1-9][0-9]*)\]|\((0|[1-9][0-9]*)\))?$").unwrap()
});

static OBJECT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]*([ \.][A-Za-z][A-Za-z0-9_]*)?$").unwrap());

static IDENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]*$").unwrap());

pub trait Validator {
    fn visit_section_header(&self, text: &str, span: &Span) -> DiagResult;
    fn visit_kvp(
        &self,
        op: KvpOperation,
        prop: &str,
        prop_span: &Span,
        text: &str,
        text_span: &Span,
    ) -> DiagResult;
    fn visit_unknown(&self, text: &str, span: &Span) -> DiagResult;
}

pub struct SimpleSyntaxValidator;

impl Validator for SimpleSyntaxValidator {
    fn visit_section_header(&self, text: &str, span: &Span) -> DiagResult {
        if OBJECT.is_match(text) {
            DiagResult::Ok
        } else {
            DiagResult::Err(vec![ReportedError {
                kind: ErrorKind::InvalidIdent,
                span: *span,
            }])
        }
    }

    fn visit_kvp(
        &self,
        _op: KvpOperation,
        prop: &str,
        prop_span: &Span,
        text: &str,
        text_span: &Span,
    ) -> DiagResult {
        let mut errs = vec![];
        if !KEY.is_match(prop) {
            match try_report_comment(prop, prop_span) {
                DiagResult::Ok => return DiagResult::Ok,
                DiagResult::None => {errs.push(ReportedError {
                    span: *prop_span,
                    kind: ErrorKind::InvalidIdent,
                })}
                DiagResult::Err(e) => {
                    errs.extend(e);
                }
            }
        }

        let mut tmp_result = None;

        match validate_property_text(text, text_span) {
            r @ (DiagResult::Ok | DiagResult::None) => tmp_result = Some(r),
            DiagResult::Err(more_errs) => errs.extend(more_errs),
        }

        if !errs.is_empty() {
            DiagResult::Err(errs)
        } else {
            tmp_result.unwrap_or(DiagResult::Ok)
        }
    }

    fn visit_unknown(&self, text: &str, span: &Span) -> DiagResult {
        let mut errs = vec![];
        match try_report_comment(text, span) {
            DiagResult::Ok => return DiagResult::Ok,
            DiagResult::None => {}
            DiagResult::Err(e) => {
                errs.extend(e);
            }
        }

        match try_report_section_error(text, span) {
            DiagResult::Ok => return DiagResult::Ok,
            DiagResult::None => {}
            DiagResult::Err(e) => {
                errs.extend(e);
            }
        }

        if errs.is_empty() {
            DiagResult::Err(vec![ReportedError {
                kind: ErrorKind::Other,
                span: *span,
            }])
        } else {
            DiagResult::Err(errs)
        }
    }
}

impl<'a> Directives<'a> {
    pub fn validate(&self, checker: &(dyn Validator + '_)) -> Vec<ReportedError> {
        let mut errs = vec![];
        for d in &self.directives {
            match d {
                Directive::SectionHeader(SectionHeader { span: _, obj_name }) => {
                    match checker.visit_section_header(&self.text[obj_name], obj_name) {
                        DiagResult::Ok | DiagResult::None => {}
                        DiagResult::Err(e) => errs.extend(e),
                    }
                }
                Directive::Kvp(Kvp {
                    span: _,
                    ident,
                    value,
                    op,
                }) => {
                    match checker.visit_kvp(*op, &self.text[ident], ident, &self.text[value], value)
                    {
                        DiagResult::Ok | DiagResult::None => {}
                        DiagResult::Err(e) => errs.extend(e),
                    }
                }
                Directive::Unknown(Unknown { span, prev_span }) => {
                    match checker.visit_unknown(&self.text[span], span) {
                        DiagResult::Ok | DiagResult::None => {}
                        DiagResult::Err(e) => {
                            errs.extend(e);
                            if let Some(prev_span) = prev_span {
                                let prev_line = &self.text[prev_span];
                                if !prev_line.ends_with(r"\\") {
                                    if let Some(beg) = prev_line.trim_end().rfind(r"\\") {
                                        let err_sp = Span(prev_span.0 + beg, span.1);
                                        errs.push(ReportedError {
                                            span: err_sp,
                                            kind: ErrorKind::SpaceAfterMultiline,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        errs
    }
}

#[derive(Clone, Debug)]
pub struct ReportedError {
    pub kind: ErrorKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ErrorKind {
    InvalidIdent,
    MalformedHeader,
    SpaceAfterMultiline,
    SlashSlashComent,
    BadValue,
    Custom(String),
    Other,
}

#[derive(Clone, Debug)]
#[must_use]
pub enum DiagResult {
    /// The checked thing was found to match something expected.
    Ok,
    /// The checked thing was not found to match something expected.
    None,
    /// The checked thing was found to be erroneous.
    Err(Vec<ReportedError>),
}

pub fn try_report_comment(text: &str, span: &Span) -> DiagResult {
    let trimmed_line = text.trim();

    if trimmed_line.starts_with(';') {
        DiagResult::Ok
    } else if trimmed_line.starts_with(r"//") {
        DiagResult::Err(vec![ReportedError {
            span: *span,
            kind: ErrorKind::SlashSlashComent,
        }])
    } else {
        DiagResult::None
    }
}

pub fn try_report_section_error(line: &str, span: &Span) -> DiagResult {
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
        DiagResult::Err(vec![ReportedError {
            span: *span,
            kind: ErrorKind::MalformedHeader,
        }])
    } else {
        DiagResult::None
    }
}

pub fn validate_property_text(text: &str, span: &Span) -> DiagResult {
    // And this is where this whole thing becomes a bit sad.
    // Basically any property text is valid because the UE3
    // config parser doesn't care about types -- it's strings
    // all the way down. In fact, even the regex used for keys
    // already excludes things the config parser happily accepts.
    //
    // As a result, this function needs to be a bit creative with guessing
    // what the user intended in order to not yield too many false positives.

    if text.is_empty() {
        return DiagResult::Ok;
    }

    // First, clear out the backslashes and direct newlines
    let mut reduced = String::new();
    let mut part_span = Span(0, text.len());

    while let Some(b' ' | b'\t') = text.as_bytes().get(part_span.0) {
        part_span.0 += 1;
    }

    while let Some(b' ' | b'\t') = text.as_bytes().get(part_span.1 - 1) {
        part_span.1 -= 1;
    }

    loop {
        match text[part_span].find(|c| matches!(c, '\r' | '\n')) {
            Some(eol) if text.get((part_span.0 + eol - 2)..(part_span.0 + eol)) == Some(r"\\") => {
                reduced.push_str(&text[(part_span.0)..(part_span.0 + eol - 2)]);
                reduced.push_str("  ");
                part_span.0 += eol;

                while matches!(
                    text.as_bytes().get(part_span.0),
                    Some(b'\t' | b'\r' | b'\n')
                ) {
                    part_span.0 += 1;
                    reduced.push(' ');
                }
            }
            Some(_) | None => {
                reduced.push_str(&text[part_span]);
                break;
            }
        }
    }

    // Then, unescape if needed
    if reduced.as_bytes().first() == Some(&b'"') {
        // TODO
        DiagResult::None
    } else {
        if matches_bool(&reduced) {
            return DiagResult::Ok;
        }

        if matches_num(&reduced) {
            return DiagResult::Ok;
        }

        if matches_ident(&reduced) {
            return DiagResult::Ok;
        }

        let mut adj_span = *span;

        if reduced.as_bytes().first() == Some(&b'(') {
            match struct_syntax::parse(&reduced) {
                Ok(_) => {
                    return DiagResult::Ok;
                }
                Err(e) => {
                    adj_span.0 += e.pos;
                    return DiagResult::Err(vec![ReportedError {
                        kind: ErrorKind::Custom(e.msg),
                        span: adj_span,
                    }]);
                }
            }
        }

        DiagResult::Err(vec![ReportedError {
            kind: ErrorKind::BadValue,
            span: adj_span,
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

#[cfg(test)]
mod tests {
    use expect_test::expect;

    use super::{KEY, OBJECT};
    use crate::{check::SimpleSyntaxValidator, parse::Directives};

    #[test]
    fn regex_key() {
        assert!(KEY.is_match("MyProperty"));
        assert!(KEY.is_match("My_Property"));
        assert!(KEY.is_match("My_Property[0]"));
        assert!(KEY.is_match("My_Property[10]"));
        assert!(KEY.is_match("My_Property01"));
        assert!(KEY.is_match("My_Property(1)"));

        assert!(!KEY.is_match("My_Property[01]"));
        assert!(!KEY.is_match("My-Property[01]"));
        assert!(!KEY.is_match("01My_Property"));
        assert!(!KEY.is_match("My_Property[1]a"));
        assert!(!KEY.is_match("My_Property{1}"));
    }

    #[test]
    fn regex_object() {
        assert!(OBJECT.is_match("MyHeader"));
        assert!(OBJECT.is_match("My_Header_1234"));
        assert!(OBJECT.is_match("MyPackage.MyHeader"));
        assert!(OBJECT.is_match("My_Name My_Object"));
        assert!(OBJECT.is_match("MyPackage345.MyClass678"));

        assert!(!OBJECT.is_match(" MyHeader"));
        assert!(!OBJECT.is_match("MyHeader "));
        assert!(!OBJECT.is_match("01NotAPackage"));
        assert!(!OBJECT.is_match("Not-A-Package"));
    }

    #[test]
    fn buggy_section_header() {
        let header = r"[MyPackage.MyClass] ";
        let expected = expect![[r#"
            Directives {
                text: "[MyPackage.MyClass] ",
                directives: [
                    Unknown(
                        Unknown {
                            span: Span(
                                0,
                                20,
                            ),
                            prev_span: None,
                        },
                    ),
                ],
            }
        "#]];
        let dirs = Directives::from_text(header);
        expected.assert_debug_eq(&dirs);

        let expected_errs = expect![[r#"
            [
                ReportedError {
                    kind: MalformedHeader,
                    span: Span(
                        0,
                        20,
                    ),
                },
            ]
        "#]];
        expected_errs.assert_debug_eq(&dirs.validate(&SimpleSyntaxValidator));
    }

    #[test]
    fn buggy_backslashes() {
        let header = r#"
+MyVariable=(Abc[0]="Def", \\ 
    )"#;
        let expected = expect![[r#"
            Directives {
                text: "\n+MyVariable=(Abc[0]=\"Def\", \\\\ \n    )",
                directives: [
                    Kvp(
                        Kvp {
                            span: Span(
                                2,
                                31,
                            ),
                            ident: Span(
                                2,
                                12,
                            ),
                            value: Span(
                                13,
                                31,
                            ),
                            op: InsertUnique,
                        },
                    ),
                    Unknown(
                        Unknown {
                            span: Span(
                                32,
                                37,
                            ),
                            prev_span: Some(
                                Span(
                                    1,
                                    31,
                                ),
                            ),
                        },
                    ),
                ],
            }
        "#]];
        let dirs = Directives::from_text(header);
        expected.assert_debug_eq(&dirs);

        let expected_errs = expect![[r#"
            [
                ReportedError {
                    kind: Custom(
                        "Expected `=`",
                    ),
                    span: Span(
                        30,
                        31,
                    ),
                },
                ReportedError {
                    kind: Other,
                    span: Span(
                        32,
                        37,
                    ),
                },
                ReportedError {
                    kind: SpaceAfterMultiline,
                    span: Span(
                        28,
                        37,
                    ),
                },
            ]
        "#]];
        expected_errs.assert_debug_eq(&dirs.validate(&SimpleSyntaxValidator))
    }

    #[test]
    fn correct_section_header() {
        let header = r"[MyPackage.MyClass]";
        let expected = expect![[r#"
            Directives {
                text: "[MyPackage.MyClass]",
                directives: [
                    SectionHeader(
                        SectionHeader {
                            span: Span(
                                0,
                                19,
                            ),
                            obj_name: Span(
                                1,
                                18,
                            ),
                        },
                    ),
                ],
            }
        "#]];
        expected.assert_debug_eq(&Directives::from_text(header));
    }
}
