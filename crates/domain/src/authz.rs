use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Role {
    Admin,
    Operator,
    Viewer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::Viewer => "viewer",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "admin" => Self::Admin,
            "operator" => Self::Operator,
            _ => Self::Viewer,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Principal {
    pub role: Role,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Read,
    Manage,
    Run,
}

#[derive(Clone, Copy, Debug)]
pub enum Resource {
    Monitor,
    Incident,
    Job,
    Maintenance,
}

pub fn can(principal: Principal, action: Action, resource: Resource) -> bool {
    match principal.role {
        Role::Admin => true,
        Role::Operator => match resource {
            Resource::Monitor | Resource::Incident | Resource::Job => true,
            Resource::Maintenance => matches!(action, Action::Read),
        },
        Role::Viewer => matches!(action, Action::Read),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(role: Role) -> Principal {
        Principal { role }
    }

    #[test]
    fn admin_can_manage_maintenance() {
        assert!(can(
            principal(Role::Admin),
            Action::Run,
            Resource::Maintenance
        ));
    }

    #[test]
    fn operator_can_manage_monitors_but_not_run_maintenance() {
        let operator = principal(Role::Operator);
        assert!(can(operator, Action::Manage, Resource::Monitor));
        assert!(!can(operator, Action::Run, Resource::Maintenance));
    }

    #[test]
    fn viewer_is_read_only() {
        let viewer = principal(Role::Viewer);
        assert!(can(viewer, Action::Read, Resource::Incident));
        assert!(!can(viewer, Action::Manage, Resource::Incident));
    }
}
