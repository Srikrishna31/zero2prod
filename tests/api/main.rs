mod admin_dashboard;
mod health_check;
mod helpers;
mod login;
mod newsletter;
mod subscriptions;
mod subscriptions_confirm;

/// Each file in tests/ folder gets compiled as its own crate. `cargo` compiles each test executable
/// in isolation and warns us if, for a specific tet file, one or more public functions in `helpers`
/// have never been invoked. This is bound to happen as your test suite grows - not all test files
/// will use all your helper methods.
/// The second option takes full advantage of that each file under tests is its own executable - we
/// can create sub-modules *scoped to a single executable*!
#[allow(dead_code)]
struct Dummy;
