use crate::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use crate::email_client::EmailClient;
use crate::startup::ApplicationBaseUrl;
use actix_web::{http::StatusCode, web, HttpResponse, ResponseError};
use anyhow::Context as anyhow_ctx;
use chrono;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use sqlx::{PgPool, Postgres, Transaction};
use tera::{Context, Tera};
use uuid::Uuid;

/// # Debug vs Display traits
/// `Debug` should return a programmer-facing representation, as faithful as possible to the underlying
/// type structure, to help with debugging (as the name implies). Almost all public types should
/// implement `Debug`.
///
/// `Display`, instead, should return a user-facing representation of the underlying type. Most types
/// do not implement `Display` and it cannot be automatically implemented with a #[ derive(Display) ]
/// attribute.
///
/// When working with errors, we can reason about the two traits as follows: `Debug` returns as much
/// information as possible while `Display` gives us a brief description of the failure we encountered
/// with the essential amount of context.
pub struct StoreTokenError(sqlx::Error);

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while trying to store a subscription token."
        )
    }
}

/// # thiserror crate and Procedural Macros
/// `thiserror::Error` is a procedural macro used via a #[ derive(/* */) ] attribute.
///
/// The macro receives, at compile-time, the definition of SubscribeError as input and returns another
/// stream of tokens as output - it *generates new Rust code*, which is then compiled into the final
/// binary.
///
/// Within the context of #[ derive(thiserror::Error) ] we get access to other attributes to achieve
/// the behavior we are looking for:
/// * #[ error(/* */) ] defines the `Display` representation of the enum variant it is applied to.
/// * #[ source ] is used to denote what should be returned as root cause in `Error::source`;
/// * #[ from ] automatically derives an implementation of From for the type it has been applied to
/// into the top-level error type(e.g. impl From<StoreTokenError> for SubscribeError {/* */}). The
/// field annotated with #[ from ] is also used as error source, saving us from having to use two
/// annotations on the same field.
#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    // Transparent delegates both `Display`'s and `source`'s implementation to the type wrapped by
    // `UnexpectedError`.
    /// We are wrapping dyn std::error::Error into a `Box` because the size of trait objects is not
    /// known at compile-time: trait objects can be used to store different types which will most
    /// likely have a different layout in memory. To use Rust's terminology, they are *unsized* - they
    /// do not implement the `Sized` marker trait. A `Box` stores the trait object itself on the heap,
    /// while we store the pointer to its heap location in `SubscribeError::UnexpectedError` the
    /// pointer itself has a known size at compile-time - problem solved, we are `Sized` again.
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// The `Error` trait is, first and foremost, a way to **semantically** mark our type as being an error.
/// It helps a reader of our codebase to immediately spot its purpose.
///
/// It is also a way for the Rust community to standardise on the minimum requirements for a **good**
/// error:
/// * it should provide different representations (`Debug` and `Display`), tuned to different audiences;
/// * it should be possible to look at the underlying cause of the error, if any (`source`).
///
impl std::error::Error for StoreTokenError {
    /// # Trait Objects
    /// Trait objects, just like generic type parameters, are a way to achieve polymorphism in Rust:
    /// invoke different implementations of the same interface. Generic types are resolved at
    /// compile-time (static dispatch), trait objects incur a runtime cost (dynamic dispatch).
    ///
    /// Why does the standard library return a trait object?
    ///
    /// It gives developers a way to access the underlying root cause of current error while keeping
    /// it *opaque*. It does not leak any information about the type of the underlying root cause -
    /// you only get access to the methods exposed by the `Error` trait: different representations
    /// (`Debug`, `Display`), the chance to go one level deeper in the **error chain** using `source`.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // The compiler transparently casts `&sqlx::Error` into a `&dyn Error`
        Some(&self.0)
    }
}

pub(in crate::routes) fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{e}\n")?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by: \n\t{cause}")?;
        current = cause.source();
    }

    Ok(())
}

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    /// This refactoring gives us a clearer separation of concerns:
    /// * `try_from` takes care of the conversion from our *wire format*(the url-decoded data
    /// collected from a HTML form) to our *domain model*(`NewSubscriber`);
    /// * `subscribe` remains in charge of generating the HTTP response to the incoming HTTP request.
    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;

        Ok(NewSubscriber { email, name })
    }
}

/// actix-web uses a *type-map* to represent its application state: a `HashMap` that stores arbitrary
/// data (using the `Any` type) against their unique type identifier(obtained via `TypeId::of`).
/// `web::Data`, when a new request comes in, computes the `TypeId` of the type you specified in the
/// signature (in this case `PgConnection`) and checks if there is a record corresponding to it in the
/// type-map. If there is one, it casts the retrieved `Any` value to the type you specified(`TypeId`
/// is unique, nothing to worry about) and passes it to your handler.
///
/// It is an interesting technique to perform what in other language ecosystems might be referred to
/// as *dependency injection*.
///
/// # Cleaning up Instrumentation Code - tracing::Instrument
///
/// We'd like all operations within subscribe to happen within the context of request_span. In other
/// words, we'd like to *wrap* the `subscribe` function in a span. `#[tracing::instrument]` creates
/// a span at the beginning of the function invocation and automatically attaches all arguments passed
/// to the function to the context of the span - in our case, `form` and `pool`. Often function arguments
/// won't be displayable on log records (e.g. `pool`) or we'd like to specify more explicitly what
/// should/how they should be captured (e.g. naming each field of `form`) - we can explicitly tell
/// `tracing` to ignore them using the skip directive.
///
/// `name` can be used to specify the message associated to the function span - if omitted, it defaults
/// to the function name.
///
/// We can also enrich the span's context using the `fields` directive. It leverages the same syntax
/// we have already for the info_span! macro.
///
/// The result is quite nice: all instrumentation concerns are visually separated by execution
/// concerns - the first are dealt with in a procedural macro that "decorates" the function declaration,
/// while the function body focuses on the actual business logic.
#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pool, email_client, base_url, templates),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]
pub async fn subscribe(
    form: web::Form<FormData>,
    // Retrieving a connection from the application state!
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
    templates: web::Data<&Tera>,
) -> Result<HttpResponse, SubscribeError> {
    // We no longer have `#[from]` for `ValidationError`, so we need to map the error explicitly.
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;
    let mut transaction = pool
        .begin()
        .await
        .context("Failed to acquire a Postgres connection from the pool")?;
    let subscriber_id = insert_subscriber(&mut transaction, &new_subscriber)
        .await
        .context("Failed to insert new subscriber in the database.")?;
    let subscription_token = generate_subscription_token();

    // The `?` operator transparently invokes the `Into` trait on our behalf - we don't need an
    // explicit `map_err` anymore.
    store_token(&mut transaction, subscriber_id, &subscription_token)
        .await
        .context("Failed to store the confirmation token for a new subscriber.")?;

    transaction
        .commit()
        .await
        .context("Failed to commit SQL transaction to store a new subscriber.")?;

    send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.as_ref().0,
        &subscription_token,
        &templates,
    )
    .await
    .context("Failed to send a confirmation mail.")?;

    Ok(HttpResponse::Ok().finish())
}

/// # Database Transcations
/// Our `POST /subscriptions` handler has grown in complexity - we are now performing two `INSERT`
/// queries against our Postgres database: one to store the details of the new subscriber, one to
/// store the newly generated subscription token.
///
/// What happens if the application crashes between those two operations?
///
/// There are three possible states for our database after an invocation of `POST /subscriptions`:
/// * a new subscriber and its token have been persisted;
/// * a new subscriber has been persisted, without a token;
/// * nothing has been persisted.
///
/// The more queries you have, the worse it gets to reason about the possible end states of our database.
///
/// Relational databases (and a few others) provide a mechanism to mitigate this issue: **transations**.
///
/// Transactions are a way to group together related operations in a single **unit of work**.
///
/// The database guarantees that all operations within a transaction will succeed or fail together:
/// the database will never be left in a state where the effect of only a subset of the queries in a
/// transaction is visible. If any of the queries within a transaction fails the database **rolls back**:
/// all changes performed by previous queries are reverted, the operation is aborted.
/// You can also explicitly trigger a rollback with the `ROLLBACK` statement.
///
/// Transactions are a deep topic: they not only provide a way to convert multiple statements into an
/// all-or-nothing operation, they also hide the effect of uncommitted changes from other queries that
/// might be running, concurrently, against the same tables.
#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber, base_url, subscription_token, templates)
)]
async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &String,
    subscription_token: &str,
    templates: &Tera,
) -> Result<(), SubscribeError> {
    // Build a confirmation link with a dynamic root
    let confirmation_link =
        format!("{base_url}/subscriptions/confirm?subscription_token={subscription_token}");

    let mut template_context = Context::new();
    template_context.insert("confirmation_link", &confirmation_link);
    let html_body = templates
        .render("confirmation.html", &template_context)
        .context("Error rendering html email template.")?;

    let plain_body = templates
        .render("confirmation.txt", &template_context)
        .context("Error rendering plain text email template.")?;

    // We are ignoring email delivery errors for now.
    email_client
        .send_email(&new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
        .context("Error sending email")?;

    Ok(())
}

/// As a rule of thumb: **Errors should be logged when they are handled.**
///
/// If your function is propagating the error upstream (e.g. using the ? operator), it should **not**
/// log the error. It can, if it makes sense, add more context to it.
///
/// If the error is propagated all the way up to the request handler, delegate logging to a dedicated
/// middleware - `tracing_actix_web::TracingLogger` in our case.
#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(new_subscriber, transaction)
)]
async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        chrono::Utc::now()
    )
    .execute(transaction)
    // Using the `?` operator to return early if the function failed, returning a sqlx::Error
    .await?;

    Ok(subscriber_id)
}

/// Generate a random 25-characters-long case-sensitive subscription token. This token should be α
/// cryptographically secure pseudo-random number generator (a CSPRNG). Every time we need to generate
/// a subscription token, we can sample a sufficiently-long sequence of alphanumeric characters.
/// Using 25 characters, we get roughly ~ 10^45 possible tokens - it should be more than enough for
/// our use case.
fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

#[tracing::instrument(
    name = "Store subscription token in the database",
    skip(subscription_token, transaction)
)]
async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    )
    .execute(transaction)
    .await
    .map_err(StoreTokenError)?;

    Ok(())
}
