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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_flag_forces_ndjson() {
        assert_eq!(detect_format(true), OutputFormat::Ndjson);
    }

    #[test]
    fn json_flag_is_idempotent() {
        assert_eq!(detect_format(true), detect_format(true));
    }
}
