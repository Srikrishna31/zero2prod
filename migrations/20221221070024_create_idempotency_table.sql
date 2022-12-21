-- Add migration script here
-- Define a Postgres composite type - i.e. a named collection of fields, the
-- equivalent of a struct in Rust code.
CREATE TYPE header_pair AS (
    name TEXT,
    value BYTEA
);

-- We could have defined an overall http_response composite type, but we would have run into a bug in sqlx which
-- in turn caused by a bug in Rust compiler. Best to avoid nested composite types for the time being.
CREATE TABLE idempotency (
    user_id uuid NOT NULL REFERENCES users(user_id),
    idempotency_key TEXT NOT NULL,
    response_status_code SMALLINT NOT NULL,
    response_headers header_pair[] NOT NULL,
    response_body BYTEA NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY(user_id, idempotency_key)
)