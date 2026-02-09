/// Configuration for spawning the Claude Code PTY process.
///
/// Owns the session-lifetime constants (command, base args, proxy URL)
/// and a generated session ID used for `--session-id` / `--resume`.
pub struct PtySpawnConfig {
    command: String,
    base_args: Vec<String>,
    base_url: String,
    /// Unique session ID generated at startup. Passed as `--session-id`
    /// on initial spawn and `--resume` on restart so we always target
    /// our own session regardless of other running instances.
    session_id: String,
}

/// Ready-to-use args and env for `PtySession::spawn()`.
pub struct SpawnParams {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// How to handle session continuation when building spawn parameters.
pub enum SessionMode {
    /// Initial spawn — use base args + `--session-id <uuid>`.
    Initial,
    /// Restart — resume our session via `--resume <uuid>`.
    Resume,
}

impl PtySpawnConfig {
    pub fn new(command: String, base_args: Vec<String>, base_url: String) -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();
        Self {
            command,
            base_args,
            base_url,
            session_id,
        }
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Build spawn parameters.
    ///
    /// - `extra_env`: additional env vars (e.g. from settings)
    /// - `extra_args`: additional CLI flags (e.g. from settings)
    /// - `session`: how to handle session continuation
    pub fn build(
        &self,
        extra_env: Vec<(String, String)>,
        extra_args: Vec<String>,
        session: SessionMode,
    ) -> SpawnParams {
        let strip = &["--continue", "--resume", "--session-id"];
        let mut args: Vec<String> = match session {
            SessionMode::Initial => {
                let mut filtered = strip_flags(&self.base_args, strip);
                filtered.push("--session-id".to_string());
                filtered.push(self.session_id.clone());
                filtered
            }
            SessionMode::Resume => {
                let mut filtered = strip_flags(&self.base_args, strip);
                filtered.push("--resume".to_string());
                filtered.push(self.session_id.clone());
                filtered
            }
        };
        args.extend(extra_args);

        let mut env = vec![("ANTHROPIC_BASE_URL".to_string(), self.base_url.clone())];
        env.extend(extra_env);

        SpawnParams { args, env }
    }
}

/// Remove flags and their values from an argument list.
///
/// Handles both `--flag value` (two separate args) and bare `--flag`
/// (no value, e.g. `--continue`).  A flag's next argument is consumed
/// as its value only if it does not start with `--`.
fn strip_flags(args: &[String], flags: &[&str]) -> Vec<String> {
    let mut filtered = Vec::new();
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if flags.contains(&arg.as_str()) {
            // If next arg looks like a value (not another flag), skip it too.
            if let Some(next) = iter.peek() {
                if !next.starts_with("--") {
                    iter.next();
                }
            }
            continue;
        }
        filtered.push(arg.clone());
    }
    filtered
}
