use termwiz::escape::parser::Parser;
use termwiz::escape::Action;

pub struct VtParser {
    parser: Parser,
}

impl VtParser {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
        }
    }

    pub fn parse(&mut self, bytes: &[u8]) -> Vec<Action> {
        let mut actions = Vec::new();

        self.parser.parse(bytes, |action| {
            actions.push(action);
        });

        actions
    }
}
