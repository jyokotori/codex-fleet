/// Hardcoded list of CLI variants the runtime supports. The order here is
/// also the canonical UI order. `wip` flags signal the CLI is reserved but
/// not yet runnable end-to-end.
pub const SUPPORTED_CLIS: &[CliInfo] = &[
    CliInfo {
        value: "codex",
        label: "Codex",
        wip: false,
    },
    CliInfo {
        value: "claude_code",
        label: "Claude Code",
        wip: true,
    },
    CliInfo {
        value: "gemini_cli",
        label: "Gemini CLI",
        wip: true,
    },
    CliInfo {
        value: "opencode",
        label: "OpenCode",
        wip: true,
    },
];

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct CliInfo {
    pub value: &'static str,
    pub label: &'static str,
    pub wip: bool,
}

pub fn cli_is_supported(value: &str) -> bool {
    SUPPORTED_CLIS.iter().any(|c| c.value == value)
}

pub fn cli_is_runnable(value: &str) -> bool {
    SUPPORTED_CLIS.iter().any(|c| c.value == value && !c.wip)
}
