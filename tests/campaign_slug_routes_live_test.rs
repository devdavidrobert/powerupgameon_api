//! Nested campaign slug routes resolve `{slug}` + `{id}` path params correctly.

use powerupgameon_api::features::campaigns::presentation::SlugIdPath;
use serde_json::json;

#[test]
fn slug_id_path_deserializes_from_nested_campaign_route() {
    let path: SlugIdPath = serde_json::from_value(json!({
        "slug": "test3",
        "id": "q-123"
    }))
    .expect("deserialize");

    assert_eq!(path.slug, "test3");
    assert_eq!(path.id, "q-123");
}

#[test]
fn slug_raffle_and_winner_path_structs_deserialize() {
    use powerupgameon_api::features::campaigns::presentation::{
        SlugRaffleIdPath, SlugWinnerIdPath,
    };

    let raffle: SlugRaffleIdPath = serde_json::from_value(json!({
        "slug": "test3",
        "raffle_id": "r-1"
    }))
    .expect("raffle path");

    assert_eq!(raffle.slug, "test3");
    assert_eq!(raffle.raffle_id, "r-1");

    let winner: SlugWinnerIdPath = serde_json::from_value(json!({
        "slug": "test3",
        "winner_id": "w-1"
    }))
    .expect("winner path");

    assert_eq!(winner.slug, "test3");
    assert_eq!(winner.winner_id, "w-1");
}
