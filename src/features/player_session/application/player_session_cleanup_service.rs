use crate::app_state::AppState;
use crate::error::ApiResult;
use crate::features::campaigns::infrastructure::CampaignPaths;
use crate::features::player_session::infrastructure::PlayerSessionRepository;

pub struct PlayerSessionCleanupService;

impl PlayerSessionCleanupService {
    /// Removes every campaign-scoped record tied to a player session so they can replay.
    pub async fn delete_all(
        state: &AppState,
        paths: &CampaignPaths,
        session_id: &str,
    ) -> ApiResult<()> {
        PlayerSessionRepository::delete_all(state, paths, session_id).await
    }
}
