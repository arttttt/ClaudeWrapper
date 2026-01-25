pub mod vt;

use termwiz::escape::Action;
use vt::VtParser;

pub struct PtyManager {
    vt_parser: VtParser,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            vt_parser: VtParser::new(),
        }
    }

    pub fn parse_output(&mut self, bytes: &[u8]) -> Vec<Action> {
        self.vt_parser.parse(bytes)
    }
}
