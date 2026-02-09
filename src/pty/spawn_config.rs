/// Configuration for spawning the Claude Code PTY process.
///
/// Owns the session-lifetime constants (command, base args, proxy URL)
/// and provides a single `build()` method that assembles the full
/// args + env for both initial spawn and restart.
pub struct PtySpawnConfig {
    command: String,
    base_args: Vec<String>,
    base_url: String,
}

/// Ready-to-use args and env for `PtySession::spawn()`.
pub struct SpawnParams {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl PtySpawnConfig {
    pub fn new(command: String, base_args: Vec<String>, base_url: String) -> Self {
        Self {
            command,
            base_args,
            base_url,
        }
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    /// Build spawn parameters.
    ///
    /// - `extra_env`: additional env vars (e.g. from settings)
    /// - `extra_args`: additional CLI flags (e.g. from settings)
    /// - `restart`: if true, ensures exactly one `--continue` in args
    pub fn build(
        &self,
        extra_env: Vec<(String, String)>,
        extra_args: Vec<String>,
        restart: bool,
    ) -> SpawnParams {
        let mut args: Vec<String> = if restart {
            let mut filtered: Vec<String> = self
                .base_args
                .iter()
                .filter(|a| *a != "--continue")
                .cloned()
                .collect();
            filtered.push("--continue".to_string());
            filtered
        } else {
            self.base_args.clone()
        };
        args.extend(extra_args);

        let mut env = vec![("ANTHROPIC_BASE_URL".to_string(), self.base_url.clone())];
        env.extend(extra_env);

        SpawnParams { args, env }
    }
}
