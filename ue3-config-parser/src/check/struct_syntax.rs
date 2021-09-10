use std::iter::FusedIterator;

#[derive(Debug, Copy, Clone)]
pub enum Token<'a> {
    LParen,
    RParen,
    LBrack,
    RBrack,
    Comma,
    Eq,
    Semi,
    Text(&'a str),
    Quoted(&'a str),
}

struct Lexer<'a> {
    text: &'a str,
    last_pos: usize,
    it: std::iter::Peekable<std::str::CharIndices<'a>>,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            last_pos: 0,
            it: text.char_indices().peekable(),
        }
    }

    /// Return the byte position of the next character that will be looked at.
    /// Whitespace might be skipped.
    fn cur_pos(&mut self) -> usize {
        self.it
            .peek()
            .map(|i| i.0)
            .unwrap_or_else(|| self.text.len())
    }

    fn continue_string(&mut self, (pos, c): (usize, char)) -> Token<'a> {
        let quoted = c == '"';
        let start = pos;
        let end;
        loop {
            match self.it.peek() {
                Some(&(p, '"')) if quoted => {
                    self.it.next();
                    end = p + 1;
                    break;
                }
                Some((p, c)) if (matches!(c, '(' | ')' | '[' | ']' | ',' | '=' | '"' | ';')) => {
                    end = *p;
                    break;
                }
                Some(_) => {
                    self.it.next();
                }
                None => {
                    end = self.text.len();
                    break;
                }
            }
        }

        if quoted {
            Token::Quoted(&self.text[start..end])
        } else {
            Token::Text(&self.text[start..end])
        }
    }
}

fn is_whitespace(i: char) -> bool {
    return matches!(i, '\t' | ' ');
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        // On EOF, we'll likely bail out with None. Because `CharIndices` is fused, subsequent calls will simply
        // simply return `None` here.
        let mut tup;
        // Skip whitespace
        while {
            self.last_pos = self.cur_pos();
            tup = self.it.next()?;
            is_whitespace(tup.1)
        } {}

        let kind = match tup.1 {
            '(' => Token::LParen,
            ')' => Token::RParen,
            '[' => Token::LBrack,
            ']' => Token::RBrack,
            ',' => Token::Comma,
            '=' => Token::Eq,
            ';' => Token::Semi,
            _ => self.continue_string(tup),
        };

        Some(kind)
    }
}

// CharIndices is Fused, we are Fused as well.
impl<'a> FusedIterator for Lexer<'a> {}

#[derive(Debug)]
pub enum PropValue<'a> {
    /// Name or 123 or 1.0 or "Something"
    Terminal(&'a str),
    /// (A="123", B[0]=Name, C=1.0)
    Struct(Struct<'a>),
    /// (A, B, C)
    Array(Array<'a>),
    /// ()
    Empty,
}

#[derive(Debug)]
pub struct PropName<'a> {
    name: &'a str,
    idx: Option<u32>,
}

#[derive(Debug)]
pub struct Struct<'a> {
    pub children: Vec<(PropName<'a>, PropValue<'a>)>,
}

#[derive(Debug)]
pub struct Array<'a> {
    pub elems: Vec<PropValue<'a>>,
}

#[derive(Debug)]
pub struct ParseError {
    pub pos: usize,
    pub msg: String,
}

impl ParseError {
    fn new(pos: usize, msg: String) -> Self {
        Self { pos, msg }
    }
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    peeked: Option<Token<'a>>,
}

impl<'a> Parser<'a> {
    fn peek(&mut self) -> Option<&Token<'a>> {
        if self.peeked.is_none() {
            self.peeked = self.lexer.next()
        }
        self.peeked.as_ref()
    }

    fn next(&mut self) -> Option<Token<'a>> {
        self.peeked.take().or_else(|| self.lexer.next())
    }

    fn pos(&mut self) -> usize {
        self.lexer.last_pos
    }
}

pub fn parse(text: &str) -> Result<Struct<'_>, ParseError> {
    let lexer = Lexer::new(text);
    let mut parser = Parser {
        lexer,
        peeked: None,
    };
    let tok = parser.next();
    match tok {
        Some(Token::LParen) => match parser.next() {
            Some(t @ Token::Text(_)) => parse_struct(&mut parser, t),
            _ => Err(ParseError::new(
                parser.pos(),
                "Expected property name".to_owned(),
            )),
        },
        _ => Err(ParseError::new(parser.pos(), "Expected `(`".to_owned())),
    }
}

/// Parse an array. `ex_token` is the first token after the opening `(`
fn parse_array<'a>(parser: &mut Parser<'a>, ex_token: Token<'a>) -> Result<Array<'a>, ParseError> {
    let mut elems = vec![];
    match ex_token {
        Token::Text(s) | Token::Quoted(s) => elems.push(PropValue::Terminal(s)),
        Token::LParen => {
            // Nested arrays don't exist, so arrays contain either terminals or structs
            elems.push(PropValue::Struct(parse_struct(parser, ex_token)?))
        }
        _ => unreachable!(),
    }

    loop {
        match parser.peek() {
            Some(Token::Comma) => {
                parser.next();
            }
            Some(Token::RParen) => {
                parser.next();
                break;
            }
            _ => {
                return Err(ParseError::new(
                    parser.pos(),
                    "expected `,` or `(`".to_owned(),
                ))
            }
        }

        match parser.next() {
            Some(Token::RParen) => {
                break;
            }
            Some(Token::Text(s) | Token::Quoted(s)) => elems.push(PropValue::Terminal(s)),
            Some(Token::LParen) => {
                // Nested arrays don't exist, so arrays contain either terminals or structs
                elems.push(PropValue::Struct(parse_struct(parser, ex_token)?))
            }
            _ => return Err(ParseError::new(parser.pos(), "expected value".to_owned())),
        }
    }

    Ok(Array { elems })
}

/// Parse a struct. `ex_token` is the first token after the opening `(`
fn parse_struct<'a>(
    parser: &mut Parser<'a>,
    ex_token: Token<'a>,
) -> Result<Struct<'a>, ParseError> {
    let mut children = vec![];

    let mut visit_token = ex_token;

    loop {
        let prop_name = match visit_token {
            Token::Text(s) => s,
            _ => unreachable!(),
        };

        let idx = match parser.peek() {
            Some(Token::LBrack) => {
                parser.next();
                if let Some(Token::Text(t)) = parser.next() {
                    match t.parse::<u32>() {
                        Ok(idx) => {
                            match parser.next() {
                                Some(Token::RBrack) => {}
                                Some(_) | None => {
                                    return Err(ParseError::new(
                                        parser.pos(),
                                        "Expected `]`".to_owned(),
                                    ))
                                }
                            }
                            Some(idx)
                        }
                        Err(_) => {
                            return Err(ParseError::new(
                                parser.pos(),
                                "Expected array index".to_owned(),
                            ))
                        }
                    }
                } else {
                    return Err(ParseError::new(
                        parser.pos(),
                        "Expected array index".to_owned(),
                    ));
                }
            }
            _ => None,
        };

        match parser.next() {
            Some(Token::Eq) => {}
            _ => return Err(ParseError::new(parser.pos(), "Expected `=`".to_owned())),
        }

        let val = match parser.next() {
            Some(Token::Text(s) | Token::Quoted(s)) => PropValue::Terminal(s),
            Some(Token::LParen) => parse_struct_or_array(parser)?,
            _ => {
                return Err(ParseError::new(
                    parser.pos(),
                    "Expected `(` or value".to_owned(),
                ))
            }
        };

        children.push((
            PropName {
                name: prop_name,
                idx,
            },
            val,
        ));

        match parser.next() {
            Some(Token::Comma) => {}
            Some(Token::RParen) => break,
            _ => {
                return Err(ParseError::new(
                    parser.pos(),
                    "Expected `,` or `)`".to_owned(),
                ))
            }
        }

        visit_token = match parser.next() {
            Some(Token::RParen) => break,
            Some(t @ Token::Text(_)) => t,
            _ => {
                return Err(ParseError::new(
                    parser.pos(),
                    "Expected `)` or name".to_owned(),
                ))
            }
        }
    }

    Ok(Struct { children })
}

fn parse_struct_or_array<'a>(parser: &mut Parser<'a>) -> Result<PropValue<'a>, ParseError> {
    let prop_token = match parser.next() {
        Some(Token::RParen) => return Ok(PropValue::Empty),
        Some(tok) => tok,
        _ => {
            return Err(ParseError::new(
                parser.pos(),
                "Expected name, value, or `)`".to_owned(),
            ))
        }
    };

    match (prop_token, parser.peek()) {
        (Token::Text(_), Some(Token::Eq | Token::LBrack)) => {
            // `prop_token` is the property name of a KVP, followed by optional index and equals sign
            parse_struct(parser, prop_token).map(PropValue::Struct)
        }
        (Token::Text(_) | Token::Quoted(_), Some(Token::Comma | Token::RParen)) => {
            // `prop_token` is a terminal followed by comma or closing paren
            parse_array(parser, prop_token).map(PropValue::Array)
        }
        (Token::LParen, Some(Token::Text(_) | Token::RParen)) => {
            // `prop_token` is the opening paren of a struct array element
            parse_array(parser, prop_token).map(PropValue::Array)
        }
        _ => Err(ParseError::new(
            parser.pos(),
            "Expected key-value pair or array value`".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use expect_test::expect;

    use super::{parse, Lexer, Token};

    #[test]
    fn test_ok_tokens() {
        let test_string = r#"(Prop1=1.0, Prop2="Abc")"#;
        let tokens = Lexer::new(test_string).collect::<Vec<Token>>();
        let expect = expect![[r#"
            [
                LParen,
                Text(
                    "Prop1",
                ),
                Eq,
                Text(
                    "1.0",
                ),
                Comma,
                Text(
                    "Prop2",
                ),
                Eq,
                Quoted(
                    "\"Abc\"",
                ),
                RParen,
            ]
        "#]];
        expect.assert_debug_eq(&tokens);

        let expect = expect![[r#"
            Ok(
                Struct {
                    children: [
                        (
                            PropName {
                                name: "Prop1",
                                idx: None,
                            },
                            Terminal(
                                "1.0",
                            ),
                        ),
                        (
                            PropName {
                                name: "Prop2",
                                idx: None,
                            },
                            Terminal(
                                "\"Abc\"",
                            ),
                        ),
                    ],
                },
            )
        "#]];
        expect.assert_debug_eq(&parse(test_string));
    }

    #[test]
    fn test_small() {
        let test_string = r#"(Prop1=1.0, Prop2[0]=(T="A", W=5),)"#;
        let tokens = Lexer::new(test_string).collect::<Vec<Token>>();
        let expect = expect![[r#"
            [
                LParen,
                Text(
                    "Prop1",
                ),
                Eq,
                Text(
                    "1.0",
                ),
                Comma,
                Text(
                    "Prop2",
                ),
                LBrack,
                Text(
                    "0",
                ),
                RBrack,
                Eq,
                LParen,
                Text(
                    "T",
                ),
                Eq,
                Quoted(
                    "\"A\"",
                ),
                Comma,
                Text(
                    "W",
                ),
                Eq,
                Text(
                    "5",
                ),
                RParen,
                Comma,
                RParen,
            ]
        "#]];
        expect.assert_debug_eq(&tokens);

        let expect = expect![[r#"
            Ok(
                Struct {
                    children: [
                        (
                            PropName {
                                name: "Prop1",
                                idx: None,
                            },
                            Terminal(
                                "1.0",
                            ),
                        ),
                        (
                            PropName {
                                name: "Prop2",
                                idx: Some(
                                    0,
                                ),
                            },
                            Struct(
                                Struct {
                                    children: [
                                        (
                                            PropName {
                                                name: "T",
                                                idx: None,
                                            },
                                            Terminal(
                                                "\"A\"",
                                            ),
                                        ),
                                        (
                                            PropName {
                                                name: "W",
                                                idx: None,
                                            },
                                            Terminal(
                                                "5",
                                            ),
                                        ),
                                    ],
                                },
                            ),
                        ),
                    ],
                },
            )
        "#]];
        expect.assert_debug_eq(&parse(test_string));
    }

    #[test]
    fn test_semi() {
        let test_string = r#"(Prop1=1.0; Prop2="Abc")"#;
        let tokens = Lexer::new(test_string).collect::<Vec<Token>>();
        let expect = expect![[r#"
            [
                LParen,
                Text(
                    "Prop1",
                ),
                Eq,
                Text(
                    "1.0",
                ),
                Semi,
                Text(
                    "Prop2",
                ),
                Eq,
                Quoted(
                    "\"Abc\"",
                ),
                RParen,
            ]
        "#]];
        expect.assert_debug_eq(&tokens);

        let expect = expect![[r#"
            Err(
                ParseError {
                    pos: 10,
                    msg: "Expected `,` or `)`",
                },
            )
        "#]];
        expect.assert_debug_eq(&parse(test_string));
    }

    #[test]
    fn exciting() {
        let test_string = r#"(ItemName="EMPGrenadeMk2", Difficulties=(0,1,2), NewCost=(ResourceCosts[0]=(ItemTemplateName="Supplies", Quantity=25)))"#;
        let tokens = Lexer::new(test_string).collect::<Vec<Token>>();
        let expect = expect![[r#"
            [
                LParen,
                Text(
                    "ItemName",
                ),
                Eq,
                Quoted(
                    "\"EMPGrenadeMk2\"",
                ),
                Comma,
                Text(
                    "Difficulties",
                ),
                Eq,
                LParen,
                Text(
                    "0",
                ),
                Comma,
                Text(
                    "1",
                ),
                Comma,
                Text(
                    "2",
                ),
                RParen,
                Comma,
                Text(
                    "NewCost",
                ),
                Eq,
                LParen,
                Text(
                    "ResourceCosts",
                ),
                LBrack,
                Text(
                    "0",
                ),
                RBrack,
                Eq,
                LParen,
                Text(
                    "ItemTemplateName",
                ),
                Eq,
                Quoted(
                    "\"Supplies\"",
                ),
                Comma,
                Text(
                    "Quantity",
                ),
                Eq,
                Text(
                    "25",
                ),
                RParen,
                RParen,
                RParen,
            ]
        "#]];
        expect.assert_debug_eq(&tokens);

        let expect = expect![[r#"
            Ok(
                Struct {
                    children: [
                        (
                            PropName {
                                name: "ItemName",
                                idx: None,
                            },
                            Terminal(
                                "\"EMPGrenadeMk2\"",
                            ),
                        ),
                        (
                            PropName {
                                name: "Difficulties",
                                idx: None,
                            },
                            Array(
                                Array {
                                    elems: [
                                        Terminal(
                                            "0",
                                        ),
                                        Terminal(
                                            "1",
                                        ),
                                        Terminal(
                                            "2",
                                        ),
                                    ],
                                },
                            ),
                        ),
                        (
                            PropName {
                                name: "NewCost",
                                idx: None,
                            },
                            Struct(
                                Struct {
                                    children: [
                                        (
                                            PropName {
                                                name: "ResourceCosts",
                                                idx: Some(
                                                    0,
                                                ),
                                            },
                                            Struct(
                                                Struct {
                                                    children: [
                                                        (
                                                            PropName {
                                                                name: "ItemTemplateName",
                                                                idx: None,
                                                            },
                                                            Terminal(
                                                                "\"Supplies\"",
                                                            ),
                                                        ),
                                                        (
                                                            PropName {
                                                                name: "Quantity",
                                                                idx: None,
                                                            },
                                                            Terminal(
                                                                "25",
                                                            ),
                                                        ),
                                                    ],
                                                },
                                            ),
                                        ),
                                    ],
                                },
                            ),
                        ),
                    ],
                },
            )
        "#]];
        expect.assert_debug_eq(&parse(test_string));
    }
}
