//! Desktop skill operations.

use ora_contracts::*;

backend_command!(
    create_skill,
    CreateSkillRequest,
    CreateSkillResponse,
    create_skill,
    "Creates one skill through the shared Backend."
);
backend_command!(
    get_skill,
    GetSkillRequest,
    GetSkillResponse,
    get_skill,
    "Gets one skill through the shared Backend."
);
backend_command!(
    list_skills,
    ListSkillsRequest,
    ListSkillsResponse,
    list_skills,
    "Lists skills through the shared Backend."
);
backend_command!(
    update_skill,
    UpdateSkillRequest,
    UpdateSkillResponse,
    update_skill,
    "Updates one skill through the shared Backend."
);
backend_command!(
    delete_skill,
    DeleteSkillRequest,
    DeleteSkillResponse,
    delete_skill,
    "Deletes one skill through the shared Backend."
);
backend_command!(
    prepare_skill_import,
    PrepareSkillImportRequest,
    PrepareSkillImportResponse,
    prepare_skill_import,
    "Prepares one skill import source into a previewed session."
);
backend_command!(
    get_skill_import,
    GetSkillImportSessionRequest,
    GetSkillImportSessionResponse,
    get_skill_import,
    "Gets one skill import session with its current progress."
);
backend_command!(
    commit_skill_import,
    CommitSkillImportRequest,
    CommitSkillImportResponse,
    commit_skill_import,
    "Accepts and freezes one skill import commit."
);
backend_command!(
    cancel_skill_import,
    CancelSkillImportRequest,
    CancelSkillImportResponse,
    cancel_skill_import,
    "Cancels one prepared skill import session."
);
