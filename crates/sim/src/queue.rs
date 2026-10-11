//! The world's single event queue (TM2, TS5), ordered by `(game-second, priority, sequence)`.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use crate::time::GameTime;

/// Tie-break between events in the same game-second: lower runs first. Each event kind gets
/// its value from one fixed table (TM2), never computed at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Priority(pub u16);

/// The full ordering key. `sequence` is unique per queue, so no two keys are equal and the
/// pop order never depends on how the heap breaks ties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventKey {
    pub time: GameTime,
    pub priority: Priority,
    pub sequence: u64,
}

#[derive(Clone, Debug)]
struct Entry<E> {
    key: EventKey,
    event: E,
}

// Ordered by key only, so event payloads need no ordering of their own.
impl<E> PartialEq for Entry<E> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<E> Eq for Entry<E> {}

impl<E> PartialOrd for Entry<E> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<E> Ord for Entry<E> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

#[derive(Clone, Debug)]
pub struct EventQueue<E> {
    heap: BinaryHeap<Reverse<Entry<E>>>,
    next_sequence: u64,
}

impl<E> Default for EventQueue<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> EventQueue<E> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            next_sequence: 0,
        }
    }

    /// Rebuilds a saved queue from `next_sequence()` and the pending `(key, event)` pairs, so
    /// the restored queue breaks ties exactly as the saved one would. `None` if two keys share
    /// a sequence or one is not below `next_sequence` (only corrupt data does that).
    #[must_use]
    pub fn from_parts(next_sequence: u64, entries: Vec<(EventKey, E)>) -> Option<Self> {
        let mut sequences: Vec<u64> = entries.iter().map(|(k, _)| k.sequence).collect();
        sequences.sort_unstable();
        let unique = sequences.windows(2).all(|w| w[0] != w[1]);
        if !unique || sequences.last().is_some_and(|&s| s >= next_sequence) {
            return None;
        }
        let heap = entries
            .into_iter()
            .map(|(key, event)| Reverse(Entry { key, event }))
            .collect();
        Some(Self {
            heap,
            next_sequence,
        })
    }

    /// Schedules `event`; among equal `(time, priority)` events, earlier pushes run first.
    pub fn push(&mut self, time: GameTime, priority: Priority, event: E) -> EventKey {
        let key = EventKey {
            time,
            priority,
            sequence: self.next_sequence,
        };
        // 2^64 pushes cannot happen in a world's lifetime; wrapping keeps this panic-free.
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.heap.push(Reverse(Entry { key, event }));
        key
    }

    /// Removes and returns the earliest event.
    pub fn pop(&mut self) -> Option<(EventKey, E)> {
        self.heap.pop().map(|Reverse(e)| (e.key, e.event))
    }

    #[must_use]
    pub fn peek_key(&self) -> Option<EventKey> {
        self.heap.peek().map(|Reverse(e)| e.key)
    }

    /// The sequence the next push will get; part of world state, since it decides future ties.
    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Every pending event in pop order, without popping. The heap's internal layout is not
    /// part of the state, so anything that hashes or saves the queue goes through this.
    #[must_use]
    pub fn sorted(&self) -> Vec<(EventKey, &E)> {
        let mut entries: Vec<_> = self
            .heap
            .iter()
            .map(|Reverse(e)| (e.key, &e.event))
            .collect();
        entries.sort_unstable_by_key(|&(key, _)| key);
        entries
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn drain<E>(q: &mut EventQueue<E>) -> Vec<(EventKey, E)> {
        std::iter::from_fn(|| q.pop()).collect()
    }

    #[test]
    fn earlier_time_runs_first() {
        let mut q = EventQueue::new();
        q.push(GameTime(20), Priority(0), "late");
        q.push(GameTime(10), Priority(9), "early");
        let order: Vec<_> = drain(&mut q).into_iter().map(|(_, e)| e).collect();
        assert_eq!(order, ["early", "late"]);
    }

    #[test]
    fn same_time_lower_priority_runs_first() {
        let mut q = EventQueue::new();
        q.push(GameTime(10), Priority(5), "daily pass");
        q.push(GameTime(10), Priority(1), "combat phase");
        let order: Vec<_> = drain(&mut q).into_iter().map(|(_, e)| e).collect();
        assert_eq!(order, ["combat phase", "daily pass"]);
    }

    #[test]
    fn full_tie_runs_in_push_order() {
        let mut q = EventQueue::new();
        for name in ["a", "b", "c", "d"] {
            q.push(GameTime(10), Priority(1), name);
        }
        let order: Vec<_> = drain(&mut q).into_iter().map(|(_, e)| e).collect();
        assert_eq!(order, ["a", "b", "c", "d"]);
    }

    #[test]
    fn push_returns_the_key_pop_reports() {
        let mut q = EventQueue::new();
        let k1 = q.push(GameTime(3), Priority(2), ());
        let k0 = q.push(GameTime(1), Priority(2), ());
        assert_eq!(q.len(), 2);
        assert_eq!(q.peek_key(), Some(k0));
        assert_eq!(q.pop().map(|(k, ())| k), Some(k0));
        assert_eq!(q.pop().map(|(k, ())| k), Some(k1));
        assert!(q.is_empty());
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn sorted_matches_pop_order_and_leaves_queue_intact() {
        let mut q = EventQueue::new();
        q.push(GameTime(5), Priority(1), "b");
        q.push(GameTime(5), Priority(0), "a");
        q.push(GameTime(9), Priority(0), "c");
        let listed: Vec<_> = q.sorted().into_iter().map(|(k, &e)| (k, e)).collect();
        assert_eq!(q.len(), 3);
        assert_eq!(q.next_sequence(), 3);
        assert_eq!(listed, drain(&mut q));
    }

    #[test]
    fn from_parts_restores_order_and_rejects_bad_sequences() {
        let mut q = EventQueue::new();
        q.push(GameTime(5), Priority(1), "b");
        q.push(GameTime(5), Priority(1), "c");
        q.push(GameTime(1), Priority(1), "a");
        let parts: Vec<_> = q.sorted().into_iter().map(|(k, &e)| (k, e)).collect();
        let mut restored = EventQueue::from_parts(q.next_sequence(), parts.clone()).unwrap();
        assert_eq!(restored.next_sequence(), 3);
        assert_eq!(drain(&mut restored), drain(&mut q));

        assert!(EventQueue::from_parts(2, parts.clone()).is_none());
        let mut dup = parts;
        dup[1].0.sequence = dup[0].0.sequence;
        assert!(EventQueue::from_parts(3, dup).is_none());
    }

    proptest! {
        /// Popping everything equals a stable sort of the pushes by `(time, priority)`.
        #[test]
        fn pop_order_is_stable_sort_by_time_then_priority(
            pushes in prop::collection::vec((0i64..50, 0u16..4), 0..200),
        ) {
            let mut q = EventQueue::new();
            for (i, &(t, p)) in pushes.iter().enumerate() {
                q.push(GameTime(t), Priority(p), i);
            }
            let got: Vec<usize> = drain(&mut q).into_iter().map(|(_, i)| i).collect();

            let mut expected: Vec<usize> = (0..pushes.len()).collect();
            expected.sort_by_key(|&i| pushes[i]);
            prop_assert_eq!(got, expected);
        }

        /// Interleaving pushes and pops still pops keys in increasing order as long as nothing
        /// is pushed before the last popped key (the world loop never does that).
        #[test]
        fn interleaved_pops_are_monotonic(
            ops in prop::collection::vec((any::<bool>(), 0i64..20, 0u16..3), 0..300),
        ) {
            let mut q = EventQueue::new();
            let mut now = GameTime(0);
            let mut last: Option<EventKey> = None;
            for (is_pop, dt, p) in ops {
                if is_pop {
                    if let Some((key, ())) = q.pop() {
                        if let Some(prev) = last {
                            prop_assert!(prev < key);
                        }
                        now = key.time;
                        last = Some(key);
                    }
                } else {
                    // Same second as the last pop: a lower priority would sort before it.
                    let p = match last {
                        Some(prev) if dt == 0 => p.max(prev.priority.0),
                        _ => p,
                    };
                    q.push(GameTime(now.0 + dt), Priority(p), ());
                }
            }
        }
    }
}
