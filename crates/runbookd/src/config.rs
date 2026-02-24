use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct RunbookConfig {
    #[serde(default)]
    pub daemon: DaemonConfig,

    pub keypad: KeypadConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
}

fn default_listen() -> String {
    "127.0.0.1:29381".to_string()
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeypadConfig {
    /// Pages are user-defined. Each page has exactly 9 slots.
    pub pages: Vec<KeypadPageConfig>,

    /// Initial page index.
    #[serde(default)]
    pub initial_page: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeypadPageConfig {
    pub name: String,
    pub slots: Vec<KeypadSlotConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeypadSlotConfig {
    /// Stable identifier for this prompt/jump gate.
    pub id: String,
    /// What to display on the LCD key.
    pub label: String,
    /// Optional second line.
    #[serde(default)]
    pub sublabel: Option<String>,
    /// What to send to Claude Code when dispatched.
    pub command: String,
}

impl RunbookConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.keypad.pages.is_empty() {
            anyhow::bail!("keypad.pages must have at least 1 page");
        }
        for (pi, p) in self.keypad.pages.iter().enumerate() {
            if p.slots.len() != 9 {
                anyhow::bail!(
                    "keypad.pages[{pi}] '{name}' must have exactly 9 slots (3x3 keypad). Got {n}.",
                    name = p.name,
                    n = p.slots.len()
                );
            }
        }
        Ok(())
    }
}

