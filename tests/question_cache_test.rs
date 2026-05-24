//! Question list cache is invalidated on admin writes so submissions under load
//! read a stable snapshot for the duration of the 45s TTL window.

use powerupgameon_api::models::question::QuestionModel;

#[test]
fn invalidate_list_cache_is_safe_for_unknown_campaigns() {
    QuestionModel::invalidate_list_cache("campaign-with-no-cache-entry");
}
