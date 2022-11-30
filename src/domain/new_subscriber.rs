use crate::domain::{SubscriberEmail, SubscriberName};

/// # Type Driven Development
/// Making an incorrect usage pattern unrepresentable, by construction is known as *type driven
/// development*. It is a powerful approach to encode the constraints of a domain we are trying to
/// model inside the type system, leaning on the compiler to make sure they are enforced.
///
/// The more expressive the type system of our programming language is, the tighter we can constrain
/// our code to only be able to represent states that are valid in the domain we are working in. This
/// particular pattern here is known as "new-type pattern" in the Rust community.
pub struct NewSubscriber {
    pub email: SubscriberEmail,
    pub name: SubscriberName,
}
