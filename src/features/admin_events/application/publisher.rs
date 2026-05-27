use crate::app_state::AppState;
use crate::features::admin_events::domain::{AdminLiveChange, AdminLiveEvent, AdminLiveTopic};
use crate::features::campaigns::presentation::CampaignContext;
use crate::features::locations::domain::GeoStatus;
use crate::models::submission::SubmissionModel;
use crate::utils::firestore::serialize_doc_data;
use chrono::Utc;
use serde_json::{json, Map, Value};

pub struct AdminLiveEventPublisher;

impl AdminLiveEventPublisher {
    pub async fn registration_added(
        state: &AppState,
        campaign_id: &str,
        session_id: &str,
        full_name: &str,
        normalized_name: &str,
        ip: &str,
        user_agent: &str,
        geo_status: GeoStatus,
        location_id: Option<String>,
        ip_geo_status: Option<String>,
    ) {
        let row = json!({
            "id": session_id,
            "fullName": full_name.to_uppercase(),
            "normalizedName": normalized_name,
            "ip": ip,
            "userAgent": user_agent,
            "registeredAt": Utc::now().to_rfc3339(),
            "status": "incomplete",
            "geoStatus": geo_status.as_str(),
            "locationId": location_id,
            "ipGeoStatus": ip_geo_status,
        });

        publish(
            state,
            campaign_id,
            AdminLiveEvent::new(
                AdminLiveTopic::Registrations,
                AdminLiveChange::Added,
                session_id,
                Some(row),
            ),
        )
        .await;
    }

    pub async fn registration_removed(state: &AppState, campaign_id: &str, session_id: &str) {
        publish(
            state,
            campaign_id,
            AdminLiveEvent::new(
                AdminLiveTopic::Registrations,
                AdminLiveChange::Removed,
                session_id,
                None,
            ),
        )
        .await;
    }

    pub async fn submission_removed(state: &AppState, campaign_id: &str, session_id: &str) {
        publish(
            state,
            campaign_id,
            AdminLiveEvent::new(
                AdminLiveTopic::Submissions,
                AdminLiveChange::Removed,
                session_id,
                None,
            ),
        )
        .await;
    }

    pub async fn submission_changed(
        state: &AppState,
        ctx: &CampaignContext,
        session_id: &str,
        change: AdminLiveChange,
    ) {
        let Some(row) = Self::load_submission_row(state, ctx, session_id).await else {
            return;
        };

        publish(
            state,
            ctx.campaign_id(),
            AdminLiveEvent::new(AdminLiveTopic::Submissions, change, session_id, Some(row)),
        )
        .await;
    }

    async fn load_submission_row(
        state: &AppState,
        ctx: &CampaignContext,
        session_id: &str,
    ) -> Option<Value> {
        let doc = SubmissionModel::find_by_id(state, &ctx.paths, session_id)
            .await
            .ok()??;
        Some(submission_row_from_doc(session_id, &doc))
    }
}

pub fn submission_row_from_doc(session_id: &str, doc: &Map<String, Value>) -> Value {
    let mut out = serialize_doc_data(doc);
    out.insert(
        "id".into(),
        doc.get("sessionId")
            .or_else(|| doc.get("id"))
            .cloned()
            .unwrap_or(json!(session_id)),
    );
    out.remove("answers");
    out.remove("ip");
    Value::Object(out)
}

async fn publish(state: &AppState, campaign_id: &str, event: AdminLiveEvent) {
    state
        .admin_events
        .publish(&state.redis, campaign_id, event)
        .await;
}
