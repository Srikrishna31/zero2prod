use crate::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use crate::email_client::EmailClient;
use crate::startup::ApplicationBaseUrl;
use actix_web::{web, HttpResponse};
use chrono;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

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
    skip(form, pool, email_client),
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
) -> HttpResponse {
    let new_subscriber = match form.0.try_into() {
        Ok(form) => form,
        Err(e) => return HttpResponse::BadRequest().body(e),
    };

    let mut transaction = match pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscriber_id = match insert_subscriber(&mut transaction, &new_subscriber).await {
        Ok(id) => id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscription_token = generate_subscription_token();
    if store_token(&mut transaction, subscriber_id, &subscription_token)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    if transaction.commit().await.is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    if send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.as_ref().0,
        &subscription_token,
    )
    .await
    .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().finish()
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
    skip(email_client, new_subscriber, base_url, subscription_token)
)]
async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &String,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    // Build a confirmation link with a dynamic root
    let confirmation_link =
        format!("{base_url}/subscriptions/confirm?subscription_token={subscription_token}");
    let plain_body = format!(
        "Welcome to our newsletter!\nVisit {confirmation_link} to confirm your subscription."
    );
    let html_body = format!(
        "Welcome to our newsletter!<bt />\
        Click <a href=\"{confirmation_link}\">here</a> to confirm your subscription."
    );

    // Send a (useless) email to the new subscriber. We are ignoring email delivery errors for now.
    email_client
        .send_email(new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
}

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
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
        // Using the `?` operator to return early if the function failed, returning a sqlx::Error
    })?;

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
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    )
    .execute(transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(())
}
