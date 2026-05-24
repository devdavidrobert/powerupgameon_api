//! Transaction-scoped Firestore reads.
//!
//! All functions here require `db` to be the clone passed into a `run_transaction`
//! closure (with `FirestoreConsistencySelector::Transaction`). Never pass
//! `state.db.client` directly — reads would fall outside the transaction read set.

use firestore::errors::{BackoffError, FirestoreError};
use firestore::FirestoreDb;
use serde_json::{Map, Value};

/// Read a subcollection document inside a Firestore transaction.
pub async fn tx_get_optional(
    db: &FirestoreDb,
    parent: &str,
    subcol: &str,
    id: &str,
) -> Result<Option<Map<String, Value>>, BackoffError<FirestoreError>> {
    db.fluent()
        .select()
        .by_id_in(subcol)
        .parent(parent)
        .obj()
        .one(id)
        .await
        .map_err(BackoffError::Permanent)
}
