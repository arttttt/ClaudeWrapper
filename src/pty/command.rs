pub fn parse_command() -> (String, Vec<String>) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    parse_command_from(args)
}

pub fn parse_command_from(args: Vec<String>) -> (String, Vec<String>) {
    let command = "claude".to_string();
    (command, args)
}

#[cfg(test)]
mod tests {
    use super::parse_command_from;

    #[test]
    fn parse_command_defaults_to_claude() {
        let (command, args) = parse_command_from(Vec::new());
        assert_eq!(command, "claude");
        assert!(args.is_empty());
    }

    #[test]
    fn parse_command_with_args() {
        let args = vec!["--debug".to_string(), "--model".to_string()];
        let (command, remaining) = parse_command_from(args);
        assert_eq!(command, "claude");
        assert_eq!(
            remaining,
            vec!["--debug".to_string(), "--model".to_string()]
        );
    }
}
