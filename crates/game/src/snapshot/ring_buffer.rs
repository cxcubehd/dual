/// A fixed-capacity circular buffer for storing snapshots indexed by tick.
///
/// Uses binary masking for O(1) lookup (capacity must be a power of two).
/// Designed for snapshot history on both server and client, covering at least
/// one full RTT of history for delta compression.
#[derive(Debug)]
pub struct SnapshotRingBuffer<T> {
    buffer: Vec<Option<T>>,
    /// Must be a power of two.
    capacity: usize,
    mask: usize,
}

impl<T> SnapshotRingBuffer<T> {
    /// Create a ring buffer with the given capacity (rounded up to next power of two).
    pub fn new(min_capacity: usize) -> Self {
        let capacity = min_capacity.next_power_of_two().max(2);
        let mut buffer = Vec::with_capacity(capacity);
        buffer.resize_with(capacity, || None);
        Self {
            buffer,
            capacity,
            mask: capacity - 1,
        }
    }

    fn index(&self, tick: u32) -> usize {
        (tick as usize) & self.mask
    }

    /// Insert a snapshot at the given tick, replacing any previous value at that slot.
    pub fn insert(&mut self, tick: u32, snapshot: T) {
        let idx = self.index(tick);
        self.buffer[idx] = Some(snapshot);
    }

    /// Get a reference to the snapshot at the given tick, if it exists.
    pub fn get(&self, tick: u32) -> Option<&T> {
        self.buffer[self.index(tick)].as_ref()
    }

    /// Get a mutable reference to the snapshot at the given tick.
    pub fn get_mut(&mut self, tick: u32) -> Option<&mut T> {
        let idx = self.index(tick);
        self.buffer[idx].as_mut()
    }

    /// Remove and return the snapshot at the given tick.
    pub fn take(&mut self, tick: u32) -> Option<T> {
        let idx = self.index(tick);
        self.buffer[idx].take()
    }

    /// Clear all slots.
    pub fn clear(&mut self) {
        for slot in &mut self.buffer {
            *slot = None;
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl<T: Clone> SnapshotRingBuffer<T> {
    /// Find the two snapshots that bracket the given tick for interpolation.
    /// Returns (older, newer) if both exist, searching backward from `tick`.
    pub fn find_interpolation_pair(&self, tick: u32) -> Option<(u32, &T, u32, &T)> {
        // Find the newer snapshot at or before tick
        let mut newer_tick = tick;
        let mut newer = None;
        for i in 0..self.capacity {
            let t = tick.wrapping_sub(i as u32);
            if let Some(snap) = self.get(t) {
                newer_tick = t;
                newer = Some(snap);
                break;
            }
        }
        let newer = newer?;

        // Find the older snapshot before newer_tick
        for i in 1..self.capacity {
            let t = newer_tick.wrapping_sub(i as u32);
            if let Some(older) = self.get(t) {
                return Some((t, older, newer_tick, newer));
            }
        }

        None
    }
}
