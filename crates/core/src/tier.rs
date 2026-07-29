use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    Free,
    General,
    Pro,
    Enterprise,
}

/// Effective tier state derived from a license after considering its
/// expiry timestamp and the caller's rollback policy (Stage 5 T5.3)
///
/// Consumers should call [`Tier::effective_state_with_policy`] rather
/// than reading the license payload directly so future rollback logic
/// (grace period, network re-check, etc) plugs in transparently
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierState {
    /// License is valid and unexpired The wrapped `Tier` reflects the
    /// license payload
    Active(Tier),
    /// License has expired but we are inside the grace window before
    /// the rollback trigger fires The wrapped `Tier` is the still-active
    /// paid tier and `until` is the wall-clock deadline at which the
    /// tier falls back to Free
    Grace { tier: Tier, until: DateTime<Utc> },
    /// License expired past any grace period The effective tier has
    /// rolled back to Free
    RolledBack,
}

impl TierState {
    /// Convenience: unwrap to the concrete tier a caller should honour
    /// right now
    #[must_use]
    pub fn effective_tier(self) -> Tier {
        match self {
            Self::Active(t) | Self::Grace { tier: t, .. } => t,
            Self::RolledBack => Tier::Free,
        }
    }
}

/// Rollback policy applied when a paid license expires (Stage 5 T5.3
/// skeleton) Concrete grace periods can be tuned per-tier later; the
/// skeleton keeps three variants so downstream state machines do not
/// need to change when policy tightens or loosens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackPolicy {
    /// Downgrade immediately at `expires_at` (default for skeleton)
    Immediate,
    /// Keep the paid tier active for `Duration` past `expires_at` before
    /// rolling back to Free (typical: 7-14 days for auto-renew retries)
    GracePeriod(Duration),
    /// Never auto-downgrade Reserved for enterprise / manual admin
    /// override use cases
    NoRollback,
}

impl Tier {
    /// Compute the effective [`TierState`] given the license's
    /// `expires_at`, the current wall-clock time, and the caller's
    /// [`RollbackPolicy`] (Stage 5 T5.3 skeleton hook)
    ///
    /// The `payload_tier` argument is the tier declared inside the
    /// verified license payload For a fresh, unexpired license this is
    /// simply passed through as `TierState::Active(payload_tier)`
    #[must_use]
    pub fn effective_state_with_policy(
        payload_tier: Self,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
        policy: RollbackPolicy,
    ) -> TierState {
        if now <= expires_at {
            return TierState::Active(payload_tier);
        }
        match policy {
            RollbackPolicy::Immediate => TierState::RolledBack,
            RollbackPolicy::NoRollback => TierState::Active(payload_tier),
            RollbackPolicy::GracePeriod(d) => {
                let deadline = expires_at + d;
                if now <= deadline {
                    TierState::Grace {
                        tier: payload_tier,
                        until: deadline,
                    }
                } else {
                    TierState::RolledBack
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Preview,
    High,
    Ultra,
}

pub struct TierLimits {
    pub daily_generations: u32,
    pub can_download: bool,
    pub force_public: bool,
    pub max_quality: Quality,
}

impl Tier {
    pub fn limits(self) -> TierLimits {
        match self {
            Self::Free => TierLimits {
                daily_generations: 5,
                can_download: false,
                force_public: false,
                max_quality: Quality::Preview,
            },
            Self::General => TierLimits {
                daily_generations: 30,
                can_download: true,
                force_public: true,
                max_quality: Quality::High,
            },
            Self::Pro => TierLimits {
                daily_generations: 100,
                can_download: true,
                force_public: false,
                max_quality: Quality::Ultra,
            },
            Self::Enterprise => TierLimits {
                daily_generations: u32::MAX,
                can_download: true,
                force_public: false,
                max_quality: Quality::Ultra,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_tier_limits() {
        let l = Tier::Free.limits();
        assert_eq!(l.daily_generations, 5);
        assert!(!l.can_download);
        assert!(!l.force_public);
        assert_eq!(l.max_quality, Quality::Preview);
    }

    #[test]
    fn general_tier_forces_public() {
        let l = Tier::General.limits();
        assert_eq!(l.daily_generations, 30);
        assert!(l.can_download);
        assert!(l.force_public);
        assert_eq!(l.max_quality, Quality::High);
    }

    #[test]
    fn pro_tier_allows_private() {
        let l = Tier::Pro.limits();
        assert_eq!(l.daily_generations, 100);
        assert!(l.can_download);
        assert!(!l.force_public);
        assert_eq!(l.max_quality, Quality::Ultra);
    }

    #[test]
    fn enterprise_tier_unlimited() {
        let l = Tier::Enterprise.limits();
        assert_eq!(l.daily_generations, u32::MAX);
        assert!(l.can_download);
        assert!(!l.force_public);
    }

    #[test]
    fn tier_serialization_roundtrip() {
        for tier in [Tier::Free, Tier::General, Tier::Pro, Tier::Enterprise] {
            let json = serde_json::to_string(&tier).unwrap();
            let deserialized: Tier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, deserialized);
        }
    }

    #[test]
    fn tier_state_effective_tier_active() {
        assert_eq!(TierState::Active(Tier::Pro).effective_tier(), Tier::Pro);
    }

    #[test]
    fn tier_state_effective_tier_grace_keeps_paid_tier() {
        let s = TierState::Grace {
            tier: Tier::Pro,
            until: Utc::now(),
        };
        assert_eq!(s.effective_tier(), Tier::Pro);
    }

    #[test]
    fn tier_state_effective_tier_rolled_back_falls_to_free() {
        assert_eq!(TierState::RolledBack.effective_tier(), Tier::Free);
    }

    #[test]
    fn effective_state_unexpired_is_active() {
        let now = Utc::now();
        let expires = now + Duration::days(30);
        let s =
            Tier::effective_state_with_policy(Tier::Pro, expires, now, RollbackPolicy::Immediate);
        assert!(matches!(s, TierState::Active(Tier::Pro)));
    }

    #[test]
    fn effective_state_expired_immediate_rolls_back() {
        let now = Utc::now();
        let expires = now - Duration::minutes(1);
        let s =
            Tier::effective_state_with_policy(Tier::Pro, expires, now, RollbackPolicy::Immediate);
        assert!(matches!(s, TierState::RolledBack));
        assert_eq!(s.effective_tier(), Tier::Free);
    }

    #[test]
    fn effective_state_expired_within_grace_keeps_paid_tier() {
        let now = Utc::now();
        let expires = now - Duration::days(3);
        let s = Tier::effective_state_with_policy(
            Tier::Pro,
            expires,
            now,
            RollbackPolicy::GracePeriod(Duration::days(7)),
        );
        assert!(matches!(
            s,
            TierState::Grace {
                tier: Tier::Pro,
                ..
            }
        ));
        assert_eq!(s.effective_tier(), Tier::Pro);
    }

    #[test]
    fn effective_state_expired_past_grace_rolls_back() {
        let now = Utc::now();
        let expires = now - Duration::days(10);
        let s = Tier::effective_state_with_policy(
            Tier::Pro,
            expires,
            now,
            RollbackPolicy::GracePeriod(Duration::days(7)),
        );
        assert!(matches!(s, TierState::RolledBack));
    }

    #[test]
    fn effective_state_no_rollback_keeps_paid_tier_forever() {
        let now = Utc::now();
        let expires = now - Duration::days(365);
        let s = Tier::effective_state_with_policy(
            Tier::Enterprise,
            expires,
            now,
            RollbackPolicy::NoRollback,
        );
        assert!(matches!(s, TierState::Active(Tier::Enterprise)));
    }

    #[test]
    fn quality_serialization_roundtrip() {
        for q in [Quality::Preview, Quality::High, Quality::Ultra] {
            let json = serde_json::to_string(&q).unwrap();
            let deserialized: Quality = serde_json::from_str(&json).unwrap();
            assert_eq!(q, deserialized);
        }
    }
}
