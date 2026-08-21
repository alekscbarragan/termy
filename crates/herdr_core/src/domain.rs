use uuid::Uuid;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }
    };
}

string_id!(SpaceId);
string_id!(AgentId);
string_id!(ConflictId);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AgentKey {
    pub space: SpaceId,
    pub agent: AgentId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentPhase {
    Starting,
    Running,
    WaitingForInput,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentCommand {
    pub program: String,
    pub argv: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RequestId(Uuid);

impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub fn into_inner(self) -> Uuid {
        self.0
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Eq, PartialEq)]
#[must_use]
pub struct Confirmed(());

impl Confirmed {
    pub fn after_user_confirmation() -> Self {
        Self(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_command_preserves_argument_tokens() {
        let command = AgentCommand {
            program: "program".to_string(),
            argv: vec![
                "a b".to_string(),
                "; echo x".to_string(),
                "$HOME".to_string(),
                "\"quoted\"".to_string(),
            ],
        };

        assert_eq!(command.program, "program");
        assert_eq!(command.argv, ["a b", "; echo x", "$HOME", "\"quoted\""]);
    }
}
