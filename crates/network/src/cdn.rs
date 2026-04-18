use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Vivaldi 座標 (2D + height)
/// ネットワーク遅延を座標空間に埋め込み、近距離ノード優先でデータ取得
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

/// ピア ID → Vivaldi 座標のマッピング（近距離ノード検索用）
pub struct PeerCoordMap {
    coords: HashMap<String, VivaldiCoord>,
}

impl PeerCoordMap {
    pub fn new() -> Self {
        Self {
            coords: HashMap::new(),
        }
    }

    /// ピアの座標を登録/更新
    pub fn update_peer(&mut self, peer_id: &str, coord: VivaldiCoord) {
        self.coords.insert(peer_id.to_string(), coord);
    }

    /// 自分の座標から最も近い N ピアを取得
    pub fn nearest(&self, my_coord: &VivaldiCoord, n: usize) -> Vec<(&str, f64)> {
        let mut distances: Vec<(&str, f64)> = self
            .coords
            .iter()
            .map(|(id, coord)| (id.as_str(), my_coord.distance(coord)))
            .collect();

        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        distances.truncate(n);
        distances
    }

    pub fn len(&self) -> usize {
        self.coords.len()
    }

    pub fn is_empty(&self) -> bool {
        self.coords.is_empty()
    }
}

impl Default for PeerCoordMap {
    fn default() -> Self {
        Self::new()
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
        assert!(
            (est - target_rtt).abs() < 1.0,
            "estimated RTT {est} should converge to {target_rtt}"
        );
    }

    #[test]
    fn vivaldi_height_non_negative() {
        let mut a = VivaldiCoord { x: 0.0, y: 0.0, height: 5.0 };
        let b = VivaldiCoord::origin();
        for _ in 0..100 {
            a.update(&b, 0.1, 0.5);
        }
        assert!(a.height >= 0.0);
    }

    #[test]
    fn peer_coord_map_empty() {
        let map = PeerCoordMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn peer_coord_map_nearest() {
        let mut map = PeerCoordMap::new();
        map.update_peer("peer-a", VivaldiCoord { x: 1.0, y: 0.0, height: 0.0 });
        map.update_peer("peer-b", VivaldiCoord { x: 10.0, y: 0.0, height: 0.0 });
        map.update_peer("peer-c", VivaldiCoord { x: 3.0, y: 0.0, height: 0.0 });

        let me = VivaldiCoord::origin();
        let nearest = map.nearest(&me, 2);
        assert_eq!(nearest.len(), 2);
        assert_eq!(nearest[0].0, "peer-a"); // closest
        assert_eq!(nearest[1].0, "peer-c");
    }

    #[test]
    fn peer_coord_map_update_overwrites() {
        let mut map = PeerCoordMap::new();
        map.update_peer("peer-a", VivaldiCoord { x: 1.0, y: 0.0, height: 0.0 });
        map.update_peer("peer-a", VivaldiCoord { x: 5.0, y: 0.0, height: 0.0 });

        assert_eq!(map.len(), 1);
        let me = VivaldiCoord::origin();
        let nearest = map.nearest(&me, 1);
        assert!((nearest[0].1 - 5.0).abs() < 1e-10);
    }

    #[test]
    fn coord_serialization_roundtrip() {
        let coord = VivaldiCoord { x: 1.5, y: -2.3, height: 0.8 };
        let json = serde_json::to_string(&coord).unwrap();
        let deserialized: VivaldiCoord = serde_json::from_str(&json).unwrap();
        assert!((coord.x - deserialized.x).abs() < 1e-10);
        assert!((coord.y - deserialized.y).abs() < 1e-10);
        assert!((coord.height - deserialized.height).abs() < 1e-10);
    }
}
