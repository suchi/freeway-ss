//! Tunnels: stretches of the road that run through a hill, lit by lamps.

/// How the lamps of the tunnels are colored, or whether there are tunnels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TunnelLighting {
    /// There are no tunnels.
    None,
    /// Each tunnel has sodium lamps or LED lamps.
    #[default]
    Mixed,
    /// Every tunnel has orange sodium lamps.
    Sodium,
    /// Every tunnel has white LED lamps.
    Led,
}

impl TunnelLighting {
    /// Every choice, in the order of the stored values 0, 1, 2, 3.
    pub const ALL: [TunnelLighting; 4] = [
        TunnelLighting::None,
        TunnelLighting::Mixed,
        TunnelLighting::Sodium,
        TunnelLighting::Led,
    ];

    /// The choice for a stored value; `None` when the value is not valid.
    pub fn from_raw(raw: Option<u32>) -> Option<Self> {
        Self::ALL.get(usize::try_from(raw?).ok()?).copied()
    }

    /// The value to store; `from_raw` reads it back.
    pub fn to_raw(self) -> u32 {
        match self {
            TunnelLighting::None => 0,
            TunnelLighting::Mixed => 1,
            TunnelLighting::Sodium => 2,
            TunnelLighting::Led => 3,
        }
    }
}
