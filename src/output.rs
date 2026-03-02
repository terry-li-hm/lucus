use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Human,
    Ndjson,
}

pub fn detect_format(json_flag: bool) -> OutputFormat {
    if json_flag || !std::io::stdout().is_terminal() {
        OutputFormat::Ndjson
    } else {
        OutputFormat::Human
    }
}
