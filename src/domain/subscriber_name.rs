use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug)]
pub struct SubscriberName(String);

/// # Type Driven Development
/// Making an incorrect usage pattern unrepresentable, by construction is known as *type driven
/// development*. It is a powerful approach to encode the constraints of a domain we are trying to
/// model inside the type system, leaning on the compiler to make sure they are enforced.
///
/// The more expressive the type system of our programming language is, the tighter we can constrain
/// our code to only be able to represent states that are valid in the domain we are working in. This
/// particular pattern here is known as "new-type pattern" in the Rust community.
pub struct NewSubscriber {
    pub email: String,
    pub name: SubscriberName,
}

impl SubscriberName {
    /// Returns an instance of `SubscriberName` if the input satisfies all our validation constraints
    /// on subscriber names. It panics otherwise.
    pub fn parse(s: String) -> Result<SubscriberName, String> {
        // `.trim()` returns a view over the input `s` without trailing whitespace-like characters.
        // `.is_empty` checks if the view contains any character.
        let is_empty_or_whitespace = s.trim().is_empty();

        // A grapheme is defined by the Unicode standard as a "user-perceived" character: `a°` is a single
        // grapheme, but it is composed of two characters (`a` and `°`).
        //
        // `graphemes` returns an iterator over the graphemes in the input `s`. `true` specifies that we
        // want to use the extended grapheme definition set, the recommended one.
        let is_too_long = s.graphemes(true).count() > 256;

        // Iterate over all characters in the input `s` to check if any of them matches one of the characters
        // in the forbidden array.
        let forbidden_characters = ['/', '(', ')', '"', '<', '>', '\\', '{', '}'];
        let contains_forbiden_characters = s.chars().any(|g| forbidden_characters.contains(&g));

        if is_empty_or_whitespace || is_too_long || contains_forbiden_characters {
            Err(format!("{s} is not a valid subscriber name."))
        } else {
            Ok(Self(s))
        }
    }
}

/// The caller gets a shared reference to the inner string. This gives the caller **read-only**
/// access, they have no way to compromise our invariants!
impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::SubscriberName;
    use claim::{assert_err, assert_ok};

    #[test]
    fn a_256_grapheme_long_name_is_valid() {
        let name = "ë".repeat(256);
        assert_ok!(SubscriberName::parse(name));
    }

    #[test]
    fn a_name_longer_than_256_graphemes_is_rejected() {
        let name = "a".repeat(257);
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn whitespace_only_names_are_rejected() {
        let name = " ".to_string();
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn empty_string_is_rejected() {
        let name = "".to_string();
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn names_containing_an_invalid_character_are_rejected() {
        for name in &['/', '(', ')', '"', '<', '>', '\\', '{', '}'] {
            let name = name.to_string();
            assert_err!(SubscriberName::parse(name));
        }
    }

    #[test]
    fn a_valid_name_is_parsed_successfully() {
        let name = "Ursula Le Guin".to_string();
        assert_ok!(SubscriberName::parse(name));
    }
}
