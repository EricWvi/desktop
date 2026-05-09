use serde::{Deserialize, Serialize};

/// Describes whether the public worktree view is active for its task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorktreeActivity {
    Inactive,
    Active,
}

/// Describes the public worktree payload shared across adapter responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub id: String,
    pub task_id: String,
    pub branch_name: Option<String>,
    pub activity: WorktreeActivity,
}

/// Carries the app-facing payload for worktree creation requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorktreeRequest {
    pub task_id: String,
    pub branch_name: Option<String>,
    pub activity: WorktreeActivity,
}

/// Returns the created worktree after a successful create request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorktreeResponse {
    pub worktree: Worktree,
}

/// Identifies which worktree to fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWorktreeRequest {
    pub worktree_id: String,
}

/// Returns one worktree payload after a successful fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWorktreeResponse {
    pub worktree: Worktree,
}

/// Requests the full visible worktree list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorktreesRequest {}

/// Returns the visible worktree list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorktreesResponse {
    pub worktrees: Vec<Worktree>,
}

/// Carries the full replacement payload for worktree updates in the first slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorktreeRequest {
    pub worktree_id: String,
    pub task_id: String,
    pub branch_name: Option<String>,
    pub activity: WorktreeActivity,
}

/// Returns the updated worktree after a successful update request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorktreeResponse {
    pub worktree: Worktree,
}

/// Identifies which worktree to delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWorktreeRequest {
    pub worktree_id: String,
}

/// Returns the deleted worktree identifier after a successful delete request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWorktreeResponse {
    pub worktree_id: String,
}

#[cfg(test)]
mod tests {
    use super::{
        CreateWorktreeRequest, CreateWorktreeResponse, DeleteWorktreeRequest,
        DeleteWorktreeResponse, GetWorktreeRequest, GetWorktreeResponse, ListWorktreesRequest,
        ListWorktreesResponse, UpdateWorktreeRequest, UpdateWorktreeResponse, Worktree,
        WorktreeActivity,
    };
    use pretty_assertions::assert_eq;
    use serde::Serialize;
    use serde_json::{Value, json};

    /// Verifies the first worktree slice serializes to frontend-friendly JSON payloads.
    #[test]
    fn serializes_worktree_contracts() {
        let worktree = Worktree {
            id: "worktree-1".to_string(),
            task_id: "task-1".to_string(),
            branch_name: Some("feature/task-handlers".to_string()),
            activity: WorktreeActivity::Active,
        };
        let create_request = CreateWorktreeRequest {
            task_id: "task-1".to_string(),
            branch_name: None,
            activity: WorktreeActivity::Inactive,
        };
        let get_request = GetWorktreeRequest {
            worktree_id: "worktree-1".to_string(),
        };
        let list_request = ListWorktreesRequest {};
        let update_request = UpdateWorktreeRequest {
            worktree_id: "worktree-1".to_string(),
            task_id: "task-2".to_string(),
            branch_name: Some("feature/updated-branch".to_string()),
            activity: WorktreeActivity::Inactive,
        };
        let delete_request = DeleteWorktreeRequest {
            worktree_id: "worktree-1".to_string(),
        };

        assert_serialized_json(
            &worktree,
            json!({
                "id": "worktree-1",
                "taskId": "task-1",
                "branchName": "feature/task-handlers",
                "activity": "active",
            }),
        );
        assert_serialized_json(
            &create_request,
            json!({
                "taskId": "task-1",
                "branchName": null,
                "activity": "inactive",
            }),
        );
        assert_serialized_json(
            &CreateWorktreeResponse {
                worktree: worktree.clone(),
            },
            json!({
                "worktree": {
                    "id": "worktree-1",
                    "taskId": "task-1",
                    "branchName": "feature/task-handlers",
                    "activity": "active",
                },
            }),
        );
        assert_serialized_json(&get_request, json!({ "worktreeId": "worktree-1" }));
        assert_serialized_json(
            &GetWorktreeResponse {
                worktree: worktree.clone(),
            },
            json!({
                "worktree": {
                    "id": "worktree-1",
                    "taskId": "task-1",
                    "branchName": "feature/task-handlers",
                    "activity": "active",
                },
            }),
        );
        assert_serialized_json(&list_request, json!({}));
        assert_serialized_json(
            &ListWorktreesResponse {
                worktrees: vec![worktree.clone()],
            },
            json!({
                "worktrees": [
                    {
                        "id": "worktree-1",
                        "taskId": "task-1",
                        "branchName": "feature/task-handlers",
                        "activity": "active",
                    },
                ],
            }),
        );
        assert_serialized_json(
            &update_request,
            json!({
                "worktreeId": "worktree-1",
                "taskId": "task-2",
                "branchName": "feature/updated-branch",
                "activity": "inactive",
            }),
        );
        assert_serialized_json(
            &UpdateWorktreeResponse { worktree },
            json!({
                "worktree": {
                    "id": "worktree-1",
                    "taskId": "task-1",
                    "branchName": "feature/task-handlers",
                    "activity": "active",
                },
            }),
        );
        assert_serialized_json(&delete_request, json!({ "worktreeId": "worktree-1" }));
        assert_serialized_json(
            &DeleteWorktreeResponse {
                worktree_id: "worktree-1".to_string(),
            },
            json!({ "worktreeId": "worktree-1" }),
        );
    }

    /// Confirms the shared worktree view remains the single reusable payload across responses.
    #[test]
    fn preserves_shared_worktree_shape_across_responses() {
        let worktree = Worktree {
            id: "worktree-1".to_string(),
            task_id: "task-1".to_string(),
            branch_name: None,
            activity: WorktreeActivity::Inactive,
        };

        assert_eq!(
            CreateWorktreeResponse {
                worktree: worktree.clone(),
            },
            CreateWorktreeResponse {
                worktree: worktree.clone(),
            }
        );
        assert_eq!(
            GetWorktreeResponse {
                worktree: worktree.clone(),
            },
            GetWorktreeResponse {
                worktree: worktree.clone(),
            }
        );
        assert_eq!(
            ListWorktreesResponse {
                worktrees: vec![worktree.clone()],
            },
            ListWorktreesResponse {
                worktrees: vec![worktree.clone()],
            }
        );
        assert_eq!(
            UpdateWorktreeResponse {
                worktree: worktree.clone(),
            },
            UpdateWorktreeResponse { worktree }
        );
    }

    /// Serializes one value and compares the full JSON payload so field names stay stable.
    fn assert_serialized_json(value: &impl Serialize, expected: Value) {
        assert_eq!(serde_json::to_value(value).unwrap(), expected);
    }
}
