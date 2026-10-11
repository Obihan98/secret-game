//! Hosts one world: the recorder, its file, and the rule that nothing reaches a client before
//! the log record behind it is durable (TS6). A crash therefore loses only what no client saw.
//!
//! The real-time loop that calls `advance_to` and `flush` (about once a second) and the
//! network side of acks and broadcasts come with the server skeleton (build-order stage 7).

use std::collections::VecDeque;
use std::path::Path;

use sim::log::{AdminAction, Recorder};
use sim::order::Order;
use sim::time::GameTime;
use sim::world::{Scenario, SimError};

use crate::store::{Store, StoreError};

/// Tells a client its order was accepted. `log_index` is the order's log record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ack {
    pub log_index: usize,
}

/// Messages held back until the log is durable up to the length each one needs.
#[derive(Debug)]
struct Outbox<T> {
    /// `(log length that must be durable, message)`, in increasing length: the log only grows.
    pending: VecDeque<(usize, T)>,
}

impl<T> Outbox<T> {
    const fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    fn push(&mut self, needs: usize, message: T) {
        self.pending.push_back((needs, message));
    }

    fn release(&mut self, durable_len: usize) -> Vec<T> {
        let ready = self
            .pending
            .partition_point(|&(needs, _)| needs <= durable_len);
        self.pending.drain(..ready).map(|(_, m)| m).collect()
    }
}

#[derive(Debug)]
pub struct Host {
    rec: Recorder,
    store: Store,
    acks: Outbox<Ack>,
}

impl Host {
    /// A new world in a new file at `path` (see `Recorder::create`).
    ///
    /// # Errors
    /// As `Recorder::create` and `Store::create`.
    pub fn create(path: &Path, seed: u64, scenario: &Scenario) -> Result<Self, StoreError> {
        let rec = Recorder::create(seed, scenario)?;
        let store = Store::create(path, &rec)?;
        Ok(Self::new(rec, store))
    }

    /// Recovers the world in `path` after a restart or crash (see `Store::open`).
    ///
    /// # Errors
    /// As `Store::open`.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let (store, rec) = Store::open(path)?;
        Ok(Self::new(rec, store))
    }

    const fn new(rec: Recorder, store: Store) -> Self {
        Self {
            rec,
            store,
            acks: Outbox::new(),
        }
    }

    /// # Errors
    /// As `Recorder::advance_to`.
    pub fn advance_to(&mut self, t: GameTime) -> Result<(), SimError> {
        self.rec.advance_to(t)
    }

    /// Applies and logs `order`. Its ack waits for the flush that makes the record durable.
    ///
    /// # Errors
    /// As `Recorder::submit`; a rejected order is neither logged nor acked.
    pub fn submit(&mut self, order: Order) -> Result<(), SimError> {
        self.rec.submit(order)?;
        let len = self.rec.log().len();
        self.acks.push(len, Ack { log_index: len - 1 });
        Ok(())
    }

    /// # Errors
    /// As `Recorder::admin`.
    pub fn admin(&mut self, action: AdminAction) -> Result<(), SimError> {
        self.rec.admin(action)
    }

    /// Commits the pending log and returns the acks that may now be sent.
    ///
    /// # Errors
    /// Any SQLite error; nothing is released and the next flush retries.
    pub fn flush(&mut self) -> Result<Vec<Ack>, StoreError> {
        let durable = self.store.persist(&self.rec)?;
        Ok(self.acks.release(durable))
    }

    /// Stores a snapshot (flushing the log with it) and returns the acks that may now be sent.
    ///
    /// # Errors
    /// Any SQLite error.
    pub fn snapshot(&mut self) -> Result<Vec<Ack>, StoreError> {
        let durable = self.store.save_snapshot(&self.rec)?;
        Ok(self.acks.release(durable))
    }

    /// # Errors
    /// As `Store::backup_to`.
    pub fn backup_to(&self, path: &Path) -> Result<(), StoreError> {
        self.store.backup_to(path)
    }

    #[must_use]
    pub const fn recorder(&self) -> &Recorder {
        &self.rec
    }

    #[must_use]
    pub const fn durable_len(&self) -> usize {
        self.store.durable_len()
    }
}

#[cfg(test)]
mod tests {
    use super::Outbox;

    #[test]
    fn outbox_releases_in_order_up_to_the_durable_length() {
        let mut o = Outbox::new();
        o.push(3, 'a');
        o.push(3, 'b');
        o.push(5, 'c');
        assert_eq!(o.release(2), []);
        assert_eq!(o.release(4), ['a', 'b']);
        assert_eq!(o.release(4), []);
        assert_eq!(o.release(9), ['c']);
    }
}
