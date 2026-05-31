use crate::net::WorldSnapshot;

#[derive(Debug)]
pub struct SnapshotBuffer {
    snapshots: Vec<Option<WorldSnapshot>>,
    capacity: usize,
}

impl SnapshotBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            snapshots: (0..capacity).map(|_| None).collect(),
            capacity,
        }
    }

    pub fn push(&mut self, snapshot: WorldSnapshot) {
        let index = (snapshot.tick as usize) % self.capacity;
        self.snapshots[index] = Some(snapshot);
    }

    pub fn get(&self, tick: u32) -> Option<&WorldSnapshot> {
        let index = (tick as usize) % self.capacity;
        self.snapshots[index].as_ref().filter(|s| s.tick == tick)
    }

    pub fn interpolation_pair(&self) -> Option<(&WorldSnapshot, &WorldSnapshot)> {
        let mut snapshots: Vec<&WorldSnapshot> =
            self.snapshots.iter().filter_map(|s| s.as_ref()).collect();
        snapshots.sort_by_key(|s| s.tick);

        if snapshots.len() >= 2 {
            let len = snapshots.len();
            Some((snapshots[len - 2], snapshots[len - 1]))
        } else {
            None
        }
    }

    pub fn latest(&self) -> Option<&WorldSnapshot> {
        self.snapshots
            .iter()
            .filter_map(|s| s.as_ref())
            .max_by_key(|s| s.tick)
    }

    pub fn clear(&mut self) {
        for slot in &mut self.snapshots {
            *slot = None;
        }
    }

    pub fn len(&self) -> usize {
        self.snapshots.iter().filter(|s| s.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o1_lookup() {
        let mut buffer = SnapshotBuffer::new(64);

        for tick in 0..100 {
            buffer.push(WorldSnapshot::new(tick, tick as u64 * 50));
        }

        assert!(buffer.get(50).is_some());
        assert_eq!(buffer.get(50).unwrap().tick, 50);
        assert!(buffer.get(30).is_none());
    }
}
