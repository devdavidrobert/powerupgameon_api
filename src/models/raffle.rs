use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::utils::firestore::millis_now;
use firestore::FirestoreQueryDirection;
use serde_json::{json, Map, Value};

const RAFFLES_SUBCOL: &str = "raffles";
const WINNERS_SUBCOL: &str = "raffle_winners";

pub struct RaffleModel;

impl RaffleModel {
    pub async fn find_all_raffles(
        state: &AppState,
        paths: &CampaignPaths,
    ) -> ApiResult<Vec<Map<String, Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        state
            .db
            .client
            .fluent()
            .select()
            .from(RAFFLES_SUBCOL)
            .parent(parent)
            .order_by([("createdAt", FirestoreQueryDirection::Descending)])
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))
    }

    pub async fn find_raffle_by_id(
        state: &AppState,
        paths: &CampaignPaths,
        id: &str,
    ) -> ApiResult<Option<Map<String, Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        state
            .db
            .client
            .fluent()
            .select()
            .by_id_in(RAFFLES_SUBCOL)
            .parent(parent)
            .obj()
            .one(id)
            .await
            .map_err(|e| ApiError::Internal(e.into()))
    }

    pub async fn find_winners_by_raffle(
        state: &AppState,
        paths: &CampaignPaths,
        raffle_id: &str,
    ) -> ApiResult<Vec<Map<String, Value>>> {
        let parent = paths.parent_str(&state.db.client)?;
        let mut winners: Vec<Map<String, Value>> = state
            .db
            .client
            .fluent()
            .select()
            .from(WINNERS_SUBCOL)
            .parent(parent)
            .filter(|q| q.field("raffleId").eq(raffle_id))
            .obj()
            .query()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        winners.sort_by(|a, b| {
            let ga = a.get("giftReceived").and_then(|v| v.as_bool()).unwrap_or(false);
            let gb = b.get("giftReceived").and_then(|v| v.as_bool()).unwrap_or(false);
            if ga == gb {
                let fa = a.get("fullName").and_then(|v| v.as_str()).unwrap_or("");
                let fb = b.get("fullName").and_then(|v| v.as_str()).unwrap_or("");
                fa.cmp(fb)
            } else if ga {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            }
        });

        Ok(winners)
    }

    pub async fn create_raffle_with_winners(
        state: &AppState,
        paths: &CampaignPaths,
        name: &str,
        winners: Vec<Map<String, Value>>,
    ) -> ApiResult<(Map<String, Value>, Vec<Map<String, Value>>)> {
        let now = millis_now();
        let raffle_id = uuid::Uuid::new_v4().to_string();
        let parent = paths.parent_str(&state.db.client)?;
        let writer = state.db.batch_writer().await.map_err(|e| ApiError::Internal(e.into()))?;
        let mut batch = writer.new_batch();

        state
            .db
            .client
            .fluent()
            .update()
            .in_col(RAFFLES_SUBCOL)
            .document_id(&raffle_id)
            .parent(parent.as_str())
            .object(&json!({
                "name": name,
                "winnerCount": winners.len(),
                "createdAt": now,
            }))
            .add_to_batch(&mut batch)
            .map_err(|e| ApiError::Internal(e.into()))?;

        let mut winner_results = Vec::new();
        for winner in winners {
            let winner_id = uuid::Uuid::new_v4().to_string();
            let mut payload = winner.clone();
            payload.insert("raffleId".into(), json!(raffle_id));
            payload.insert("raffleName".into(), json!(name));
            payload.insert("giftReceived".into(), json!(false));
            payload.insert("selectedAt".into(), json!(now));
            state
                .db
                .client
                .fluent()
                .update()
                .in_col(WINNERS_SUBCOL)
                .document_id(&winner_id)
                .parent(parent.as_str())
                .object(&payload)
                .add_to_batch(&mut batch)
                .map_err(|e| ApiError::Internal(e.into()))?;
            payload.insert("id".into(), json!(winner_id));
            winner_results.push(payload);
        }

        batch.write().await.map_err(|e| ApiError::Internal(e.into()))?;

        let mut raffle = Map::new();
        raffle.insert("id".into(), json!(raffle_id));
        raffle.insert("name".into(), json!(name));
        raffle.insert("winnerCount".into(), json!(winner_results.len()));
        raffle.insert("createdAt".into(), json!(now));

        Ok((raffle, winner_results))
    }

    pub async fn update_gift_received(
        state: &AppState,
        paths: &CampaignPaths,
        winner_id: &str,
        gift_received: bool,
    ) -> ApiResult<()> {
        let parent = paths.parent_str(&state.db.client)?;
        state
            .db
            .client
            .fluent()
            .update()
            .in_col(WINNERS_SUBCOL)
            .document_id(winner_id)
            .parent(parent)
            .object(&json!({ "giftReceived": gift_received }))
            .execute::<Map<String, Value>>()
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        Ok(())
    }
}
