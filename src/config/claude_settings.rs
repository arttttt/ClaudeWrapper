use std::collections::HashMap;

/// Unique identifier for each setting.
///
/// Adding a new setting: add a variant here + entry in `builtin_registry()`.
/// The `as_str()` value is used as TOML key — once published, do not rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingId {
    AgentTeams,
}

impl SettingId {
    /// Stable TOML key for persistence.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentTeams => "agent_teams",
        }
    }

    /// All variants for iteration.
    pub fn all() -> &'static [SettingId] {
        &[Self::AgentTeams]
    }

    /// Parse from TOML key. Unknown keys return `None` (forward compat).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "agent_teams" => Some(Self::AgentTeams),
            _ => None,
        }
    }
}

/// UI section for visual grouping of settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSection {
    Experimental,
}

impl SettingSection {
    /// Display label for the section header in the popup.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Experimental => "Experimental Features",
        }
    }
}

/// Self-contained definition of a single setting.
///
/// Contains everything needed for UI display, env injection, CLI args,
/// and persistence. Adding a new setting = one `SettingDef` in the registry.
pub struct SettingDef {
    pub id: SettingId,
    pub label: &'static str,
    pub description: &'static str,
    pub section: SettingSection,
    /// Env var name to set when enabled (OS-level string).
    pub env_var: Option<&'static str>,
    /// Env var value when enabled.
    pub env_value: &'static str,
    /// CLI flag when enabled (e.g. "--verbose").
    pub cli_flag: Option<&'static str>,
    pub default: bool,
}

/// Registry-based settings manager.
///
/// Owns both the definitions (immutable) and current values.
/// UI snapshots are derived from this; changes are applied back via snapshots.
pub struct ClaudeSettingsManager {
    registry: Vec<SettingDef>,
    values: HashMap<SettingId, bool>,
}

impl ClaudeSettingsManager {
    /// Create with the builtin registry and default values.
    pub fn new() -> Self {
        Self {
            registry: builtin_registry(),
            values: HashMap::new(),
        }
    }

    /// Get the current value for a setting (falls back to default).
    pub fn get(&self, id: SettingId) -> bool {
        self.values
            .get(&id)
            .copied()
            .unwrap_or_else(|| self.default_for(id))
    }

    /// Set the value for a setting.
    pub fn set(&mut self, id: SettingId, value: bool) {
        self.values.insert(id, value);
    }

    /// Toggle the value for a setting.
    pub fn toggle(&mut self, id: SettingId) {
        let current = self.get(id);
        self.set(id, !current);
    }

    /// Access the ordered registry (determines UI order).
    pub fn registry(&self) -> &[SettingDef] {
        &self.registry
    }

    /// Build env vars for all enabled settings.
    pub fn to_env_vars(&self) -> Vec<(String, String)> {
        let mut vars = Vec::new();
        for def in &self.registry {
            if self.get(def.id) {
                if let Some(env_var) = def.env_var {
                    vars.push((env_var.to_string(), def.env_value.to_string()));
                }
            }
        }
        vars
    }

    /// Build CLI args for all enabled settings. Includes `--continue` if any
    /// setting with a CLI flag is active.
    pub fn to_cli_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        for def in &self.registry {
            if self.get(def.id) {
                if let Some(flag) = def.cli_flag {
                    args.push(flag.to_string());
                }
            }
        }
        args
    }

    /// Serialize to TOML-friendly map (SettingId → string key).
    pub fn to_toml_map(&self) -> HashMap<String, bool> {
        let mut map = HashMap::new();
        for def in &self.registry {
            map.insert(def.id.as_str().to_string(), self.get(def.id));
        }
        map
    }

    /// Load values from a TOML map. Unknown keys are ignored (forward compat).
    pub fn load_from_toml(&mut self, map: &HashMap<String, bool>) {
        for (key, &value) in map {
            if let Some(id) = SettingId::parse(key) {
                self.values.insert(id, value);
            }
        }
    }

    /// Check if current values differ from a saved snapshot.
    pub fn is_dirty(&self, saved: &HashMap<SettingId, bool>) -> bool {
        for def in &self.registry {
            let current = self.get(def.id);
            let saved_val = saved.get(&def.id).copied().unwrap_or(def.default);
            if current != saved_val {
                return true;
            }
        }
        false
    }

    /// Create UI snapshots from current state (for SettingsIntent::Load).
    pub fn to_snapshots(&self) -> Vec<SettingsFieldSnapshot> {
        self.registry
            .iter()
            .map(|def| SettingsFieldSnapshot {
                id: def.id,
                label: def.label,
                description: def.description,
                section: def.section,
                value: self.get(def.id),
            })
            .collect()
    }

    /// Apply UI snapshots back to values (after user edits).
    pub fn apply_snapshots(&mut self, fields: &[SettingsFieldSnapshot]) {
        for field in fields {
            self.set(field.id, field.value);
        }
    }

    /// Snapshot current values for dirty-checking later.
    pub fn snapshot_values(&self) -> HashMap<SettingId, bool> {
        let mut map = HashMap::new();
        for def in &self.registry {
            map.insert(def.id, self.get(def.id));
        }
        map
    }

    fn default_for(&self, id: SettingId) -> bool {
        self.registry
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.default)
            .unwrap_or(false)
    }
}

impl Default for ClaudeSettingsManager {
    fn default() -> Self {
        Self::new()
    }
}

/// UI-friendly snapshot of a single setting (used in MVI state).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsFieldSnapshot {
    pub id: SettingId,
    pub label: &'static str,
    pub description: &'static str,
    pub section: SettingSection,
    pub value: bool,
}

fn builtin_registry() -> Vec<SettingDef> {
    vec![SettingDef {
        id: SettingId::AgentTeams,
        label: "Agent Teams",
        description: "Enable multi-agent collaboration (experimental)",
        section: SettingSection::Experimental,
        env_var: Some("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"),
        env_value: "1",
        cli_flag: None,
        default: false,
    }]
}
