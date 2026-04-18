use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    Free,
    General,
    Pro,
    Enterprise,
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
    fn quality_serialization_roundtrip() {
        for q in [Quality::Preview, Quality::High, Quality::Ultra] {
            let json = serde_json::to_string(&q).unwrap();
            let deserialized: Quality = serde_json::from_str(&json).unwrap();
            assert_eq!(q, deserialized);
        }
    }
}
