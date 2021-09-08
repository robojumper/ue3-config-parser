use std::fs::read_to_string;
use std::io;

use ue3_config_parser::check::SimpleSyntaxValidator;
use walkdir::{DirEntry, WalkDir};

fn is_ini(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.ends_with(".ini"))
        .unwrap_or(false)
}

fn main() {
    let dir = std::env::args().nth(1).expect("missing directory");
    let walker = WalkDir::new(dir).into_iter();
    for entry in walker {
        let entry = match entry {
            Ok(d) => d,
            Err(e) => {
                println!("{:?}", e);
                continue;
            }
        };

        if !is_ini(&entry) {
            continue;
        }

        let contents = match read_to_string(entry.path()) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                println!("{:?}: Invalid UTF-8", entry.path());
                continue;
            }
            Err(e) => {
                println!("{:?}: I/O Error {:?}", entry.path(), e);
                continue;
            }
        };

        let d = ue3_config_parser::parse::Directives::from_text(&contents);
        for u in &d.validate(&SimpleSyntaxValidator) {
            println!("{:?}: {:?} {:?}", entry.path(), u.kind, u.span);
            println!("{}", &(*contents)[u.span]);
        }
    }
}
