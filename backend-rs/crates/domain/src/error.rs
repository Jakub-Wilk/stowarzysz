use std::borrow::Cow;
use std::fmt;

/// A business rule was violated by the input.
///
/// `code` is a stable, machine-readable key (`snake_case`) that clients may branch on;
/// `message` is the Polish text shown to the user; `field` names the offending input field, if
/// the violation is tied to one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainError {
    pub code: &'static str,
    pub message: Cow<'static, str>,
    pub field: Option<Cow<'static, str>>,
}

impl DomainError {
    /// A violation not tied to a particular field.
    pub fn new(code: &'static str, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            code,
            message: message.into(),
            field: None,
        }
    }

    /// Attach the violation to an input field (e.g. `"title"` or `"items.2.amount"`).
    #[must_use]
    pub fn on(mut self, field: impl Into<Cow<'static, str>>) -> Self {
        self.field = Some(field.into());
        self
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.field {
            Some(field) => write!(f, "{field}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for DomainError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_the_field_when_present() {
        let plain = DomainError::new("title_required", "Podaj tytuł.");
        assert_eq!(plain.to_string(), "Podaj tytuł.");
        assert_eq!(plain.clone().on("title").to_string(), "title: Podaj tytuł.");
        assert_eq!(plain.field, None);
    }
}
