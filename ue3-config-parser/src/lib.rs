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
    pub class_name: Span,
}
#[derive(Clone, Copy, Debug)]
pub struct Comment {
    pub span: Span,
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
    Comment(Comment),
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

    fn index(&self, index: Span) -> &Self::Output {
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

fn validate_ident(text: &str, span: &Span, path: bool) -> Result<(), usize> {
    for (idx, c) in text[*span].char_indices() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '(' | ')' | '[' | ']' => {}
            '.' | ' ' if path => {},
            _ => return Err(idx),
        }
    }
    Ok(())
}

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
                        class_name: Span(span.0 + 1, span.1 - 1),
                    }));
                } else {
                    let trim_span = {
                        let mut start = span.0;
                        let start = loop {
                            match text.as_bytes().get(start) {
                                Some(b' ' | b'\t') => start += 1,
                                _ => break start,
                            }
                        };
                        Span(start, span.1)
                    };
                    let trim_line = &text[trim_span];
                    if trim_line.as_bytes().first() == Some(&b';') {
                        directives.push(Directive::Comment(Comment { span: trim_span }));
                    } else if let Some(p) = trim_line.find('=') {
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

    pub fn validate(&self) -> Vec<ReportedError> {
        let mut errs = vec![];
        for d in &self.directives{
            match d {
                Directive::Comment(_) => { /* comments are fine */ }
                Directive::SectionHeader(SectionHeader { span: _, class_name }) => {
                    if let Err(_idx) = validate_ident(self.text, class_name, true) {
                        errs.push(ReportedError {
                            span: *class_name,
                            kind: ErrorKind::InvalidIdent,
                        });
                    }
                }
                Directive::Kvp(Kvp {
                    span: _,
                    ident,
                    value: _,
                    op: _,
                }) => {
                    if let Err(_idx) = validate_ident(self.text, ident, false) {
                        errs.push(ReportedError {
                            span: *ident,
                            kind: ErrorKind::InvalidIdent,
                        });
                    }
                }
                Directive::Unknown(Unknown { span, prev_span }) => {
                    let line = &self.text[*span];
                    let trimmed_line = line.trim();
                    let mut reported = false;
                    if matches!(
                        (
                            trimmed_line.as_bytes().first(),
                            trimmed_line.as_bytes().last()
                        ),
                        (Some(b'['), Some(b']'))
                    ) {
                        errs.push(ReportedError {
                            span: *span,
                            kind: ErrorKind::MalformedHeader,
                        });
                        reported = true;
                    } else if trimmed_line.starts_with(r"//") {
                        errs.push(ReportedError { span: *span, kind: ErrorKind::SlashSlashComent });
                        reported = true;
                    } else if let Some(prev_span) = prev_span {
                        let prev_line = &self.text[*prev_span];
                        if !prev_line.ends_with(r"\\") {
                            if let Some(beg) = prev_line.trim_end().rfind(r"\\") {
                                let err_sp = Span(prev_span.0 + beg, span.1);
                                errs.push(ReportedError { span: err_sp, kind: ErrorKind::SpaceAfterMultiline });
                                reported = true;
                            }
                        }
                    }

                    if !reported {
                        errs.push(ReportedError {
                            span: *span,
                            kind: ErrorKind::Other,
                        });
                    }
                }
            }
        }

        errs
    }
}

#[cfg(test)]
mod tests {
    use expect_test::expect;

    use crate::Directives;

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
        expected.assert_debug_eq(&Directives::from_text(header));
    }

    #[test]
    fn buggy_backslashes() {
        let header = r#"
[CorrectHeader MyClass]
+MyVariable=(Abc[0]="Def", \\ 
    )"#;
        let expected = expect![[r#"
            Directives {
                text: "\n[CorrectHeader MyClass]\n+MyVariable=(Abc[0]=\"Def\", \\\\ \n    )",
                directives: [
                    SectionHeader(
                        SectionHeader {
                            span: Span(
                                1,
                                24,
                            ),
                            class_name: Span(
                                2,
                                23,
                            ),
                        },
                    ),
                    Kvp(
                        Kvp {
                            span: Span(
                                26,
                                55,
                            ),
                            ident: Span(
                                26,
                                36,
                            ),
                            value: Span(
                                37,
                                55,
                            ),
                            op: InsertUnique,
                        },
                    ),
                    Unknown(
                        Unknown {
                            span: Span(
                                56,
                                61,
                            ),
                            prev_span: Some(
                                Span(
                                    25,
                                    55,
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
                        52,
                        61,
                    ),
                },
            ]
        "#]];
        expected_errs.assert_debug_eq(&dirs.validate())
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
                            class_name: Span(
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
