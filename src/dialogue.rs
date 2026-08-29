use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

/// The displayed speaker for a single line of dialogue.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Speaker {
    NoFive,
    NoOne,
    NoTwo,
    System,
}

impl Speaker {
    /// The stable lower-case label used by dialogue UI.
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::NoFive => "no. five",
            Self::NoOne => "no. one",
            Self::NoTwo => "no. two",
            Self::System => "system",
        }
    }
}

/// One ordered, displayed dialogue line.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DialogueLine {
    pub speaker: Speaker,
    pub text: String,
}

/// A human-editable collection of terminal conversations keyed by dialogue ID.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DialogueCatalog {
    pub conversations: BTreeMap<String, Vec<DialogueLine>>,
}

#[derive(Debug)]
pub enum DialogueError {
    InvalidFilename,
    Io(io::Error),
    Ron(ron::error::SpannedError),
    EmptyId,
    EmptyConversation { id: String },
    UnknownId { id: String },
}

impl fmt::Display for DialogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFilename => write!(f, "invalid dialogue filename"),
            Self::Io(error) => error.fmt(f),
            Self::Ron(error) => write!(f, "malformed dialogue RON: {error}"),
            Self::EmptyId => write!(f, "dialogue catalog contains an empty conversation ID"),
            Self::EmptyConversation { id } => {
                write!(
                    f,
                    "dialogue conversation '{id}' must contain at least one line"
                )
            }
            Self::UnknownId { id } => write!(f, "unknown dialogue ID '{id}'"),
        }
    }
}

impl Error for DialogueError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Ron(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for DialogueError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Validates catalog IDs and requires every conversation to have a line.
pub fn validate_catalog(catalog: &DialogueCatalog) -> Result<(), DialogueError> {
    for (id, lines) in &catalog.conversations {
        if id.trim().is_empty() {
            return Err(DialogueError::EmptyId);
        }
        if lines.is_empty() {
            return Err(DialogueError::EmptyConversation { id: id.clone() });
        }
    }
    Ok(())
}

impl DialogueCatalog {
    /// Returns a conversation after validating that its ID exists and has lines.
    pub fn conversation(&self, id: &str) -> Result<&[DialogueLine], DialogueError> {
        if id.trim().is_empty() {
            return Err(DialogueError::EmptyId);
        }
        let lines = self
            .conversations
            .get(id)
            .ok_or_else(|| DialogueError::UnknownId { id: id.to_owned() })?;
        if lines.is_empty() {
            return Err(DialogueError::EmptyConversation { id: id.to_owned() });
        }
        Ok(lines)
    }
}

/// Parses and validates a RON dialogue catalog.
pub fn parse_dialogue_catalog(contents: &str) -> Result<DialogueCatalog, DialogueError> {
    let catalog = ron::de::from_str(contents).map_err(DialogueError::Ron)?;
    validate_catalog(&catalog)?;
    Ok(catalog)
}

/// Builds a safe path below `assets/dialogue` for a dialogue catalog name.
pub fn dialogue_path(name: &str) -> Result<PathBuf, DialogueError> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(DialogueError::InvalidFilename);
    }
    Ok(Path::new("assets/dialogue").join(format!("{name}.ron")))
}

/// Loads and validates a named dialogue catalog from `assets/dialogue/<name>.ron`.
pub fn load_dialogue_catalog(name: &str) -> Result<DialogueCatalog, DialogueError> {
    let contents = fs::read_to_string(dialogue_path(name)?)?;
    parse_dialogue_catalog(&contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_preserves_line_order() {
        let catalog = parse_dialogue_catalog(
            r#"(
                conversations: {
                    "intro": [
                        (speaker: NoOne, text: "first"),
                        (speaker: NoFive, text: "second"),
                    ],
                },
            )"#,
        )
        .expect("valid catalog parses");

        let lines = catalog.conversation("intro").expect("known ID exists");
        assert_eq!(lines[0].text, "first");
        assert_eq!(lines[1].speaker, Speaker::NoFive);
    }

    #[test]
    fn validation_rejects_empty_id_and_conversation() {
        let empty_id = DialogueCatalog {
            conversations: BTreeMap::from([(" ".to_owned(), vec![line()])]),
        };
        assert!(matches!(
            validate_catalog(&empty_id),
            Err(DialogueError::EmptyId)
        ));

        let empty_conversation = DialogueCatalog {
            conversations: BTreeMap::from([("intro".to_owned(), vec![])]),
        };
        assert!(matches!(
            validate_catalog(&empty_conversation),
            Err(DialogueError::EmptyConversation { id }) if id == "intro"
        ));
    }

    #[test]
    fn malformed_ron_is_reported() {
        let error = parse_dialogue_catalog("(conversations: [)").expect_err("RON is malformed");
        assert!(matches!(error, DialogueError::Ron(_)));
        assert!(error.to_string().contains("malformed dialogue RON"));
    }

    #[test]
    fn each_speaker_has_its_required_display_label() {
        assert_eq!(Speaker::NoFive.display_label(), "no. five");
        assert_eq!(Speaker::NoOne.display_label(), "no. one");
        assert_eq!(Speaker::NoTwo.display_label(), "no. two");
        assert_eq!(Speaker::System.display_label(), "system");
    }

    #[test]
    fn dialogue_filenames_cannot_traverse_assets_directory() {
        for name in ["", "../story", "nested/story", r"nested\\story"] {
            assert!(matches!(
                dialogue_path(name),
                Err(DialogueError::InvalidFilename)
            ));
        }
        assert_eq!(
            dialogue_path("story").unwrap(),
            PathBuf::from("assets/dialogue/story.ron")
        );
    }

    fn line() -> DialogueLine {
        DialogueLine {
            speaker: Speaker::System,
            text: "test".to_owned(),
        }
    }
}
