use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use ue3_config_parser::{
    check::{ErrorKind, SimpleSyntaxValidator},
    parse::Directives,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Annotations {
    pub annots: Box<[Annotation]>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Annotation {
    pub err: String,
    pub line: u32,
    pub col: u32,
    pub eline: u32,
    pub ecol: u32,
}

#[wasm_bindgen]
pub fn check(input: &str) -> JsValue {
    JsValue::from_serde(&check_inner(input)).unwrap()
}

fn check_inner(input: &str) -> Annotations {
    let directives = Directives::from_text(input);
    let errors = directives.validate(&SimpleSyntaxValidator);

    let lookup = line_col::LineColLookup::new(input);
    let mut annots = vec![];

    for e in errors {
        let (line, col) = lookup.get_by_cluster(e.span.0);
        let (eline, ecol) = lookup.get_by_cluster(e.span.1);
        let err = match &e.kind {
            ErrorKind::InvalidIdent => "Invalid identifier",
            ErrorKind::MalformedHeader => "Invalid header. The first character of a header line must be `[` and the last must be `]`.",
            ErrorKind::SpaceAfterMultiline => "Unrecognized directive (space after backslashes)",
            ErrorKind::SlashSlashComent => "UnrealScript-style comment (please use `;`)",
            ErrorKind::BadValue => "Bad Value",
            ErrorKind::Custom(s) => s,
            ErrorKind::Other => "Invalid config directive",
        };

        annots.push(Annotation {
            err: err.into(),
            line: line as u32,
            col: col as u32,
            eline: eline as u32,
            ecol: ecol as u32,
        });
    }

    Annotations {
        annots: annots.into_boxed_slice(),
    }
}

#[wasm_bindgen]
pub fn init() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

#[cfg(test)]
mod test {
    use expect_test::expect;

    #[test]
    fn test_weird() {
        let input = r#"+SpawnDistributionLists=(ListID="DefaultLeaders", \\
    SpawnDistribution[0]=(Template="AdvWraithM1", 		MinForceLevel=3, 	MaxForceLevel=7, 	MaxCharactersPerGroup=1, 	SpawnWeight=5), \\
    )"#;

        let expected = expect![[r#"
            Annotations {
                annots: [],
            }
        "#]];
        expected.assert_debug_eq(&super::check_inner(input));
    }

    #[test]
    fn test_sigma() {
        let input = r#"[Package.CorrectHeader]
+MyArray=(Entry[0]="Abc", \\
), \\
), \\"#;
        let expected = expect![[r#"
            Annotations {
                annots: [
                    Annotation {
                        err: "Trailing \\\\ without following line",
                        line: 4,
                        col: 1,
                        eline: 4,
                        ecol: 6,
                    },
                ],
            }
        "#]];
        expected.assert_debug_eq(&super::check_inner(input));
    }
}
