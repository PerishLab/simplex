use crate::{Binary, Type};

impl Type {
    pub const fn bytes(&self) -> u32 {
        match self {
            Self::Void => 0,
            Self::Integer { bits, .. } => (*bits as u32).div_ceil(8),
            Self::Float { bits } => (*bits as u32) / 8,
            Self::Pointer => 8,
            Self::Pair => 16,
            Self::Aggregate { size, .. } => *size,
        }
    }

    pub const fn alignment(&self) -> u32 {
        match self {
            Self::Void => 1,
            Self::Integer { bits, .. } => {
                let raw = (*bits as u32).div_ceil(8);
                match raw {
                    0 | 1 => 1,
                    2 => 2,
                    3 | 4 => 4,
                    _ => 8,
                }
            }
            Self::Float { bits } => match (*bits as u32) / 8 {
                0 => 1,
                other => other,
            },
            Self::Pointer | Self::Pair => 8,
            Self::Aggregate { align, packed, .. } => match *packed {
                true => 1,
                false => match *align {
                    0 => 1,
                    other => other,
                },
            },
        }
    }

    pub const fn valid(&self) -> bool {
        match self {
            Self::Void | Self::Pointer | Self::Pair => true,
            Self::Integer { bits, .. } => matches!(*bits, 1 | 8 | 16 | 32 | 64 | 128),
            Self::Float { bits } => matches!(*bits, 32 | 64),
            Self::Aggregate { align, .. } => *align >= 1 && align.is_power_of_two(),
        }
    }

    pub const fn concrete(&self) -> bool {
        !matches!(self, Self::Void) && self.valid()
    }

    pub const fn word(&self) -> bool {
        matches!(
            self,
            Self::Integer { .. } | Self::Float { .. } | Self::Pointer
        )
    }

    pub const fn atomic(&self) -> bool {
        matches!(self, Self::Integer { .. } | Self::Pointer)
    }
}

impl Binary {
    pub const fn comparison(self) -> bool {
        matches!(
            self,
            Self::Eq | Self::Ne | Self::Lt | Self::Le | Self::Gt | Self::Ge
        )
    }
}
