use serde::{Deserialize, Serialize};

/// Vivaldi 座標 (2D + height)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VivaldiCoord {
    pub x: f64,
    pub y: f64,
    pub height: f64,
}

impl VivaldiCoord {
    pub fn origin() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            height: 0.0,
        }
    }

    /// 2 ノード間の推定 RTT (ms)
    pub fn distance(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt() + self.height + other.height
    }

    /// RTT 観測に基づく座標更新
    pub fn update(&mut self, other: &Self, rtt_ms: f64, weight: f64) {
        let est = self.distance(other);
        let error = rtt_ms - est;

        let dx = other.x - self.x;
        let dy = other.y - self.y;
        let norm = (dx * dx + dy * dy).sqrt().max(1e-6);

        self.x += weight * error * (dx / norm);
        self.y += weight * error * (dy / norm);
        self.height = (self.height + weight * error * 0.1).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vivaldi_distance() {
        let a = VivaldiCoord { x: 0.0, y: 0.0, height: 1.0 };
        let b = VivaldiCoord { x: 3.0, y: 4.0, height: 2.0 };
        let dist = a.distance(&b);
        assert!((dist - 8.0).abs() < 1e-10);
    }

    #[test]
    fn vivaldi_distance_symmetric() {
        let a = VivaldiCoord { x: 1.0, y: 2.0, height: 0.5 };
        let b = VivaldiCoord { x: 4.0, y: 6.0, height: 1.0 };
        assert!((a.distance(&b) - b.distance(&a)).abs() < 1e-10);
    }

    #[test]
    fn vivaldi_self_distance_is_height_sum() {
        let a = VivaldiCoord { x: 5.0, y: 3.0, height: 2.0 };
        // distance to self = 0 (euclidean) + height + height
        assert!((a.distance(&a) - 4.0).abs() < 1e-10);
    }

    #[test]
    fn vivaldi_origin() {
        let o = VivaldiCoord::origin();
        assert!((o.x).abs() < 1e-10);
        assert!((o.y).abs() < 1e-10);
        assert!((o.height).abs() < 1e-10);
    }

    #[test]
    fn vivaldi_update_converges() {
        let mut a = VivaldiCoord::origin();
        let b = VivaldiCoord { x: 10.0, y: 0.0, height: 0.0 };
        let target_rtt = 10.0;

        for _ in 0..100 {
            a.update(&b, target_rtt, 0.1);
        }

        let est = a.distance(&b);
        assert!((est - target_rtt).abs() < 1.0, "estimated RTT {est} should converge to {target_rtt}");
    }

    #[test]
    fn vivaldi_height_non_negative() {
        let mut a = VivaldiCoord { x: 0.0, y: 0.0, height: 5.0 };
        let b = VivaldiCoord::origin();
        // Small RTT should reduce height, but never below 0
        for _ in 0..100 {
            a.update(&b, 0.1, 0.5);
        }
        assert!(a.height >= 0.0);
    }
}
