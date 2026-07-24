use ora_domain::AgentCli;
use std::path::{Path, PathBuf};

/// Resolves the OpenCode executable from the fixed per-user installation layout.
pub(super) fn opencode_executable(home_directory: &Path) -> PathBuf {
    home_directory
        .join(".opencode")
        .join("bin")
        .join("opencode")
}

/// Resolves the NGA executable independently so its layout can evolve without coupling providers.
pub(super) fn nga_executable(home_directory: &Path) -> PathBuf {
    home_directory.join(".nga").join("bin").join("nga")
}

/// Resolves the CodeAgentCLI executable independently from the other provider paths.
pub(super) fn code_agent_cli_executable(home_directory: &Path) -> PathBuf {
    home_directory
        .join(".codeagentcli")
        .join("bin")
        .join("codeagentcli")
}

/// Selects the separately-defined fixed executable path for one immutable session provider.
pub(super) fn executable_for(agent_cli: AgentCli, home_directory: &Path) -> PathBuf {
    match agent_cli {
        AgentCli::OpenCode => opencode_executable(home_directory),
        AgentCli::Nga => nga_executable(home_directory),
        AgentCli::CodeAgentCli => code_agent_cli_executable(home_directory),
    }
}
