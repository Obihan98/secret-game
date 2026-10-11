//! Every item here breaks a determinism ban; check.ps1 expects clippy to name each one.

#[must_use]
pub fn hash_map() -> std::collections::HashMap<u32, u32> {
    std::collections::HashMap::new()
}

#[must_use]
pub fn hash_set() -> std::collections::HashSet<u32> {
    std::collections::HashSet::new()
}

#[must_use]
pub fn instant() -> std::time::Instant {
    std::time::Instant::now()
}

#[must_use]
pub fn system_time() -> std::time::SystemTime {
    std::time::SystemTime::now()
}
