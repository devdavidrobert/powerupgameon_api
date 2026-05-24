use crate::features::campaigns::domain::{Campaign, StaggerMode};
use crate::features::inventory::domain::InventorySlot;

pub struct InventoryService;

impl InventoryService {
    pub fn releasable_now(campaign: &Campaign, slot: &InventorySlot, now_ms: i64) -> i64 {
        if slot.total_quantity <= 0 {
            return 0;
        }

        match campaign.stagger_mode {
            StaggerMode::Immediate => slot.total_quantity,
            StaggerMode::Linear => Self::linear_releasable(campaign, slot.total_quantity, now_ms),
            StaggerMode::Stepped => {
                Self::stepped_releasable(campaign, slot.total_quantity, now_ms)
            }
        }
    }

    fn linear_releasable(campaign: &Campaign, total: i64, now_ms: i64) -> i64 {
        let Some(start) = campaign.challenge_start_time else {
            return total;
        };
        let Some(end) = campaign.challenge_end_time else {
            return total;
        };
        if end <= start {
            return total;
        }
        let clamped = now_ms.clamp(start, end);
        let elapsed = clamped - start;
        let duration = end - start;
        ((total as f64) * (elapsed as f64 / duration as f64)).floor() as i64
    }

    fn stepped_releasable(campaign: &Campaign, total: i64, now_ms: i64) -> i64 {
        let Some(steps) = &campaign.stagger_schedule else {
            return Self::linear_releasable(campaign, total, now_ms);
        };
        if steps.is_empty() {
            return 0;
        }

        let mut percent = 0.0_f64;
        for step in steps {
            if now_ms >= step.release_at {
                percent = percent.max(step.release_percent);
            }
        }
        ((total as f64) * percent).floor() as i64
    }
}
