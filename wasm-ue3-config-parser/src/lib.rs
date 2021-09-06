use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Annotations {
    pub annots: Box<[Annotation]>,
}

#[derive(Serialize, Deserialize)]
pub struct Annotation {
    pub err: String,
    pub line: u32,
    pub col: u32,
    pub eline: u32,
    pub ecol: u32,
}

#[wasm_bindgen]
pub fn check(input: &str) -> JsValue {
    let directives = ue3_config_parser::Directives::from_text(input);
    let errors = directives.validate();

    let lookup = line_col::LineColLookup::new(input);
    let mut annots = vec![];

    for e in errors {
        let (line, col) = lookup.get_by_cluster(e.span.0);
        let (eline, ecol) = lookup.get_by_cluster(e.span.1);
        let err = match e.kind {
            ue3_config_parser::ErrorKind::InvalidIdent => "Invalid identifier",
            ue3_config_parser::ErrorKind::MalformedHeader => "Invalid header. The first character of a header line must be `[` and the last must be `]`.",
            ue3_config_parser::ErrorKind::SpaceAfterMultiline => "Unrecognized directive (space after backslashes)",
            ue3_config_parser::ErrorKind::SlashSlashComent => "UnrealScript-style comment (please use `;`)",
            ue3_config_parser::ErrorKind::Other => "Invalid config directive",
        };

        annots.push(Annotation { err: err.into(), line: line as u32, col: col as u32, eline: eline as u32, ecol: ecol as u32 });
    }

    let annots = Annotations { annots: annots.into_boxed_slice() };

    JsValue::from_serde(&annots).unwrap()
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
