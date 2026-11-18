//! Lists of names in a sentence.

/// `Ola`, `Ola i Kuba`, `Ola, Kuba i Ania`.
pub fn join_names<S: AsRef<str>>(names: &[S]) -> String {
    match names {
        [] => String::new(),
        [only] => only.as_ref().to_owned(),
        [rest @ .., last] => {
            let rest: Vec<&str> = rest.iter().map(AsRef::as_ref).collect();
            format!("{} i {}", rest.join(", "), last.as_ref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_like_a_sentence() {
        assert_eq!(join_names::<&str>(&[]), "");
        assert_eq!(join_names(&["Ola"]), "Ola");
        assert_eq!(join_names(&["Ola", "Kuba"]), "Ola i Kuba");
        assert_eq!(join_names(&["Ola", "Kuba", "Ania"]), "Ola, Kuba i Ania");
    }
}
