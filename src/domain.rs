use unicode_segmentation::UnicodeSegmentation;

pub struct SubscriberName(String);

pub struct NewSubscriber {
    pub email: String,
    pub name: SubscriberName,
}

impl SubscriberName {
    /// Returns an instance of `SubscriberName` if the input satisfies all our validation constraints
    /// on subscriber names. It panics otherwise.
    pub fn parse(s: String) -> SubscriberName {
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
            panic!("{s} is not a valid subscriber name.")
        } else {
            Self(s)
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
