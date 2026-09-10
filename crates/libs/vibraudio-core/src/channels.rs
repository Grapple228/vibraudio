#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Channels {
    Mono = 1,
    Stereo = 2,
}

impl Channels {
    #[inline(always)]
    pub const fn as_usize(self) -> usize {
        self as usize
    }

    #[inline(always)]
    pub const fn as_u16(self) -> u16 {
        match self {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        }
    }

    #[inline(always)]
    pub const fn from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::Mono),
            2 => Some(Self::Stereo),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Channels::Mono => "mono",
            Channels::Stereo => "stereo",
        }
    }
}

impl std::fmt::Display for Channels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
