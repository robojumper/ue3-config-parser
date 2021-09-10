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
                        while test_line.ends_with(r"\\") && l_index < lines.len() - 1 {
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
}
