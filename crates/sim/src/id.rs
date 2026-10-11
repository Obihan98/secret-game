//! Typed IDs: indices into the flat state tables (TS5). A newtype per table keeps a
//! `ProvinceId` from indexing the nation table.

macro_rules! typed_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl $name {
            /// Position in the table this ID indexes.
            #[must_use]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

typed_id!(
    /// A province (MP2).
    ProvinceId
);
typed_id!(
    /// A nation.
    NationId
);
typed_id!(
    /// An army (MP7).
    ArmyId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_the_raw_value() {
        assert_eq!(ProvinceId(7).index(), 7);
        assert_eq!(NationId(u32::MAX).index(), u32::MAX as usize);
    }

    #[test]
    fn ids_order_by_raw_value() {
        assert!(ProvinceId(1) < ProvinceId(2));
    }
}
