//! Desktop git identity operations.

use ora_contracts::*;

backend_command!(
    get_git_identity,
    GetGitIdentityRequest,
    GitIdentityResponse,
    read_git_identity,
    "Reads the host's global Git identity through the shared Backend."
);
