//! 一次投影给出这条 Companion 会话落在哪个工作区。

use crate::core::workspace::Workspace;

/// 工作区、这条会话的 inbox，或没有根。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompanionWorkspace<'a> {
    Workspace(&'a Workspace),
    Inbox { session_id: String },
    NoRoot,
}

/// 拒绝能逃出 inbox 目录的会话 id。
pub(crate) fn session_can_own_inbox(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 180
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// 显式工作区优先，其次是会话绑定，否则是这条会话的 inbox。
pub(crate) fn project_companion_workspace<'a>(
    explicit_workspace_id: Option<&str>,
    bound_workspace_id: Option<&str>,
    workspaces: &'a [Workspace],
    session_id: Option<&str>,
) -> CompanionWorkspace<'a> {
    if let Some(workspace) = find_workspace(workspaces, explicit_workspace_id)
        .or_else(|| find_workspace(workspaces, bound_workspace_id))
    {
        return CompanionWorkspace::Workspace(workspace);
    }
    match session_id
        .map(str::trim)
        .filter(|id| session_can_own_inbox(id))
    {
        Some(session_id) => CompanionWorkspace::Inbox {
            session_id: session_id.to_string(),
        },
        None => CompanionWorkspace::NoRoot,
    }
}

fn find_workspace<'a>(workspaces: &'a [Workspace], id: Option<&str>) -> Option<&'a Workspace> {
    let id = id.map(str::trim).filter(|id| !id.is_empty())?;
    workspaces.iter().find(|workspace| workspace.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn workspace(id: &str) -> Workspace {
        Workspace {
            id: id.to_string(),
            name: id.to_string(),
            root: PathBuf::from(id),
            description: None,
            source: None,
            created_at: Utc::now(),
            last_used_at: Utc::now(),
            pinned: false,
            archived: false,
            sort_order: 0,
        }
    }

    #[test]
    fn explicit_workspace_id_is_the_companion_workspace() {
        let workspaces = vec![workspace("desktop"), workspace("bound")];
        let projected = project_companion_workspace(
            Some("desktop"),
            Some("bound"),
            &workspaces,
            Some("session-1"),
        );
        assert!(matches!(
            projected,
            CompanionWorkspace::Workspace(workspace) if workspace.id == "desktop"
        ));
    }

    #[test]
    fn unknown_explicit_id_uses_the_session_binding() {
        let workspaces = vec![workspace("bound")];
        let projected = project_companion_workspace(
            Some("missing"),
            Some("bound"),
            &workspaces,
            Some("session-1"),
        );
        assert!(matches!(
            projected,
            CompanionWorkspace::Workspace(workspace) if workspace.id == "bound"
        ));
    }

    #[test]
    fn unbound_session_projects_to_its_inbox() {
        let projected = project_companion_workspace(None, None, &[], Some("session-1"));
        assert!(matches!(
            projected,
            CompanionWorkspace::Inbox { session_id } if session_id == "session-1"
        ));
    }

    #[test]
    fn padded_session_id_still_owns_its_inbox() {
        let projected = project_companion_workspace(None, None, &[], Some(" session-1 "));
        assert!(matches!(
            projected,
            CompanionWorkspace::Inbox { session_id } if session_id == "session-1"
        ));
    }

    #[test]
    fn missing_session_has_no_root() {
        let projected = project_companion_workspace(None, Some("missing"), &[], None);
        assert_eq!(projected, CompanionWorkspace::NoRoot);
    }

    #[test]
    fn session_id_that_cannot_own_an_inbox_has_no_root() {
        let projected = project_companion_workspace(None, None, &[], Some("../victim"));
        assert_eq!(projected, CompanionWorkspace::NoRoot);
    }
}
