use powerupgameon_api::utils::helpers::{
    fisher_yates_shuffle, normalize_name, submission_identity_from_registration,
};
use serde_json::{json, Map};

#[test]
fn submission_identity_from_registration_returns_stored_names() {
    let mut reg = Map::new();
    reg.insert("fullName".into(), json!("JANE DOE"));
    reg.insert("normalizedName".into(), json!("jane doe"));
    let (full, norm) = submission_identity_from_registration(&reg).unwrap();
    assert_eq!(full, "JANE DOE");
    assert_eq!(norm, "jane doe");
}

#[test]
fn submission_identity_from_registration_rejects_missing_fields() {
    let reg = Map::new();
    assert!(submission_identity_from_registration(&reg).is_err());

    let mut partial = Map::new();
    partial.insert("fullName".into(), json!("JANE DOE"));
    assert!(submission_identity_from_registration(&partial).is_err());
}

#[test]
fn normalize_name_collapses_whitespace_and_lowercases() {
    assert_eq!(normalize_name("  Alice   SMITH  "), "alice smith");
}

#[test]
fn fisher_yates_shuffle_preserves_elements() {
    let input: Vec<i32> = (0..20).collect();
    let shuffled = fisher_yates_shuffle(input.clone());
    assert_eq!(shuffled.len(), input.len());
    let mut sorted = shuffled.clone();
    sorted.sort();
    assert_eq!(sorted, input);
}
