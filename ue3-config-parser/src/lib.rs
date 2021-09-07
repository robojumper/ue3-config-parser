use once_cell::sync::Lazy;
use regex::Regex;
use std::ops::Index;

#[derive(Clone, Copy, Debug)]
pub struct Span(pub usize, pub usize);

#[derive(Clone, Copy, Debug)]
pub struct Identifier {
    pub span: Span,
}

#[derive(Clone, Copy, Debug)]
pub struct SectionHeader {
    pub span: Span,
    pub obj_name: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvpOperation {
    Set,
    Insert,
    InsertUnique,
    Remove,
    Clear,
}

impl From<u8> for KvpOperation {
    fn from(value: u8) -> Self {
        match value {
            b'+' => KvpOperation::InsertUnique,
            b'.' => KvpOperation::Insert,
            b'-' => KvpOperation::Remove,
            b'!' => KvpOperation::Clear,
            _ => KvpOperation::Set,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Kvp {
    pub span: Span,
    pub ident: Span,
    pub value: Span,
    pub op: KvpOperation,
}
#[derive(Clone, Copy, Debug)]
pub struct Unknown {
    pub span: Span,
    pub prev_span: Option<Span>,
}

#[derive(Clone, Copy, Debug)]
pub enum Directive {
    SectionHeader(SectionHeader),
    Kvp(Kvp),
    Unknown(Unknown),
}

#[derive(Clone, Debug)]
pub struct Directives<'a> {
    pub text: &'a str,
    pub directives: Vec<Directive>,
}

impl Index<Span> for str {
    type Output = str;

    #[inline]
    fn index(&self, index: Span) -> &Self::Output {
        &self[index.0..index.1]
    }
}

impl Index<&Span> for str {
    type Output = str;

    #[inline]
    fn index(&self, index: &Span) -> &Self::Output {
        &self[index.0..index.1]
    }
}

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
    Other,
}

static IDENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]?$").unwrap());

static KEY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[A-Za-z][A-Za-z0-9_]*(\[(0|[1-9][0-9]*)\]|\((0|[1-9][0-9]*)\))?$").unwrap()
});

static OBJECT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]*([ \.][A-Za-z][A-Za-z0-9_]*)?$").unwrap());

impl<'a> Directives<'a> {
    pub fn from_text(text: &'a str) -> Self {
        // Split our input text into lines
        let lines = {
            let mut lines = vec![];
            let mut remaining = text;
            let mut offset = 0;
            while !remaining.is_empty() {
                match remaining.find(|c| matches!(c, '\r' | '\n')) {
                    Some(p) => {
                        lines.push(Span(offset, offset + p));
                        offset += p;
                        remaining = &remaining[p..];
                        while remaining.starts_with(|c| matches!(c, '\r' | '\n')) {
                            offset += 1;
                            remaining = &remaining[1..];
                        }
                    }
                    None => {
                        lines.push(Span(offset, offset + remaining.len()));
                        break;
                    }
                }
            }
            lines
        };

        // Then parse directives
        let directives = {
            let mut directives = vec![];
            let mut l_index = 0;
            while l_index < lines.len() {
                let span = lines[l_index];
                let line = &text[span];

                if matches!(
                    (line.as_bytes().first(), line.as_bytes().last()),
                    (Some(b'['), Some(b']'))
                ) {
                    directives.push(Directive::SectionHeader(SectionHeader {
                        span,
                        obj_name: Span(span.0 + 1, span.1 - 1),
                    }));
                } else {
                    let mut trim_span = span;
                    while let Some(b' ' | b'\t') = text.as_bytes().get(trim_span.0) {
                        trim_span.0 += 1;
                    }
                    let trim_line = &text[trim_span];
                    if let Some(p) = trim_line.find('=') {
                        let mut prop_span = Span(trim_span.0, trim_span.0 + p);
                        while let Some(b' ' | b'\t') = text.as_bytes().get(prop_span.1 - 1) {
                            prop_span.1 -= 1;
                        }
                        let op = trim_line.as_bytes()[0].into();
                        let mut value_span = Span(trim_span.0 + p + 1, trim_span.1);

                        let mut test_line = trim_line;
                        while test_line.ends_with(r"\\") {
                            l_index += 1;
                            let next_span = lines[l_index];
                            test_line = &text[next_span];
                            value_span.1 = next_span.1;
                        }
                        if op != KvpOperation::Set {
                            prop_span.0 += 1;
                        }
                        directives.push(Directive::Kvp(Kvp {
                            ident: prop_span,
                            op,
                            span: Span(prop_span.0, value_span.1),
                            value: value_span,
                        }));
                    } else if !line
                        .as_bytes()
                        .iter()
                        .all(|c| matches!(c, b'\r' | b'\n' | b'\t' | b' '))
                    {
                        directives.push(Directive::Unknown(Unknown {
                            span,
                            prev_span: l_index.checked_sub(1).and_then(|i| lines.get(i).copied()),
                        }));
                    }
                }

                l_index += 1;
            }

            Directives { text, directives }
        };

        directives
    }

    pub fn validate(&self, strict: bool) -> Vec<ReportedError> {
        let mut errs = vec![];
        for d in &self.directives {
            match d {
                Directive::SectionHeader(SectionHeader {
                    span: _,
                    obj_name: class_name,
                }) => {
                    if !OBJECT.is_match(&self.text[class_name]) {
                        errs.push(ReportedError {
                            span: *class_name,
                            kind: ErrorKind::InvalidIdent,
                        });
                    }
                }
                Directive::Kvp(Kvp {
                    span: _,
                    ident,
                    value,
                    op: _,
                }) => {
                    if !KEY.is_match(&self.text[ident]) {
                        errs.push(ReportedError {
                            span: *ident,
                            kind: ErrorKind::InvalidIdent,
                        });
                    }

                    match validate_property_text(self.text, value, strict) {
                        Diag::Ok | Diag::None => {}
                        Diag::Err(e) => {
                            errs.extend(e);
                        }
                    }
                }
                Directive::Unknown(Unknown { span, prev_span }) => {
                    match try_report_comment(self.text, span) {
                        Diag::Ok => continue,
                        Diag::None => {}
                        Diag::Err(e) => {
                            errs.extend(e);
                            continue;
                        }
                    }

                    match try_report_section_error(self.text, span) {
                        Diag::Ok => continue,
                        Diag::None => {}
                        Diag::Err(e) => {
                            errs.extend(e);
                            continue;
                        }
                    }

                    if let Some(prev_span) = prev_span {
                        let prev_line = &self.text[prev_span];
                        if !prev_line.ends_with(r"\\") {
                            if let Some(beg) = prev_line.trim_end().rfind(r"\\") {
                                let err_sp = Span(prev_span.0 + beg, span.1);
                                errs.push(ReportedError {
                                    span: err_sp,
                                    kind: ErrorKind::SpaceAfterMultiline,
                                });
                                continue;
                            }
                        }
                    }

                    errs.push(ReportedError {
                        span: *span,
                        kind: ErrorKind::Other,
                    });
                }
            }
        }

        errs
    }
}

#[derive(Clone, Debug)]
enum Diag {
    Ok,
    None,
    Err(Vec<ReportedError>),
}

fn try_report_comment(text: &str, span: &Span) -> Diag {
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

fn try_report_section_error(text: &str, span: &Span) -> Diag {
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

fn validate_property_text(text: &str, span: &Span, strict: bool) -> Diag {
    // And this is where this whole thing becomes a bit sad.
    // Basically any property text is valid because the UE3
    // config parser doesn't care about types -- it's strings
    // all the way down. In fact, even the regex used for keys
    // already excludes things the config parser happily accepts.
    //
    // As a result, this function needs to be a bit creative with guessing
    // what the user intended in order to not yield too many false positives.

    let unescaped_text = &text[span];
    let trimmed_unescaped_text = unescaped_text.trim();

    // Name, simple string, enum
    if IDENT.is_match(trimmed_unescaped_text) {
        return Diag::Ok;
    }

    return Diag::None;
}

#[cfg(test)]
mod tests {
    use expect_test::expect;

    use crate::{Directives, KEY, OBJECT};

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
        expected_errs.assert_debug_eq(&dirs.validate(false))
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
                    kind: SpaceAfterMultiline,
                    span: Span(
                        28,
                        37,
                    ),
                },
            ]
        "#]];
        expected_errs.assert_debug_eq(&dirs.validate(false))
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
