use crate::models::CurrentUser;
use async_graphql::{Error, Result};
use uptime_domain::{Principal, can};

pub use uptime_domain::{Action, Resource};

pub fn require(
    user: Option<&CurrentUser>,
    action: Action,
    resource: Resource,
) -> Result<CurrentUser> {
    let user = user
        .cloned()
        .ok_or_else(|| Error::new("authentication required"))?;
    if can(
        Principal {
            role: user.parsed_role(),
        },
        action,
        resource,
    ) {
        Ok(user)
    } else {
        Err(Error::new("not authorized"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uptime_domain::Role;
    use uuid::Uuid;

    fn user(role: Role) -> CurrentUser {
        CurrentUser {
            id: Uuid::new_v4(),
            email: "user@example.com".to_string(),
            name: "User".to_string(),
            role: role.as_str().to_string(),
        }
    }

    #[test]
    fn admin_can_manage_maintenance() {
        assert!(require(Some(&user(Role::Admin)), Action::Run, Resource::Maintenance).is_ok());
    }

    #[test]
    fn operator_can_manage_monitors_but_not_run_maintenance() {
        let operator = user(Role::Operator);
        assert!(require(Some(&operator), Action::Manage, Resource::Monitor).is_ok());
        assert!(require(Some(&operator), Action::Run, Resource::Maintenance).is_err());
    }

    #[test]
    fn viewer_is_read_only() {
        let viewer = user(Role::Viewer);
        assert!(require(Some(&viewer), Action::Read, Resource::Incident).is_ok());
        assert!(require(Some(&viewer), Action::Manage, Resource::Incident).is_err());
    }
}
