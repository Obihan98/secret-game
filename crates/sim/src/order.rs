//! Player orders: the inputs the log records (TM9). A validated order is applied at the
//! world's current second; an invalid one changes nothing and is never logged.
//!
//! Stub (build-order stage 3): one order, just enough to exercise the log and replay. Real
//! orders come with their systems; the wire format is the protocol crate's job (stage 7).

use crate::codec::{DecodeError, Reader, Writer};
use crate::id::{NationId, ProvinceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Hands a province to another nation: a sudden change, so both nations' income rates are
    /// recomputed in the same second (TM2).
    TransferProvince { province: ProvinceId, to: NationId },
}

impl Order {
    /// Tags are part of the file format: never reuse or renumber one.
    pub(crate) fn encode(self, w: &mut Writer) {
        match self {
            Self::TransferProvince { province, to } => {
                w.put_u8(0);
                w.put_u32(province.0);
                w.put_u32(to.0);
            }
        }
    }

    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0 => Ok(Self::TransferProvince {
                province: ProvinceId(r.u32()?),
                to: NationId(r.u32()?),
            }),
            tag => Err(DecodeError::BadTag(tag)),
        }
    }
}
