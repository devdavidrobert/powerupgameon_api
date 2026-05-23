use powerupgameon_api::utils::helpers::{fisher_yates_shuffle, normalize_name};

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
