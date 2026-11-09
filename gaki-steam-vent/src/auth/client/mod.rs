use gaki_steam_vent_crypto::MachineId as RawMachineId;
use gaki_steam_vent_proto::protobuf::Enum;
use gaki_steam_vent_proto::steammessages_auth_steamclient::EAuthTokenPlatformType;
use serde::{Deserialize, Serialize};

/// Information about the client to present to steam when authenticating
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(from = "RawClientInfo")]
#[serde(into = "RawClientInfo")]
#[non_exhaustive]
pub struct ClientInfo {
    pub name: String,
    pub machine_id: MachineId,
    pub platform_type: EAuthTokenPlatformType,
    pub os: Os,
}

impl ClientInfo {
    pub fn new(name: String) -> Self {
        ClientInfo {
            name,
            ..Self::default()
        }
    }
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self {
            name: "DESKTOP-VENT".into(),
            machine_id: Default::default(),
            platform_type: EAuthTokenPlatformType::k_EAuthTokenPlatformType_SteamClient,
            os: Default::default(),
        }
    }
}

/// Client OS type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Os {
    Web,
    Linux,
    MacOs,
    #[default]
    Windows,
}

impl From<Os> for i32 {
    fn from(value: Os) -> Self {
        match value {
            Os::Web => -700,
            Os::Linux => -203,
            Os::MacOs => -102,
            Os::Windows => 0,
        }
    }
}

impl TryFrom<i32> for Os {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            -700 => Ok(Os::Web),
            -203 => Ok(Os::Linux),
            -102 => Ok(Os::MacOs),
            0 => Ok(Os::Windows),
            _ => Err(()),
        }
    }
}

/// Unique identifier for the machine
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineId {
    id: RawMachineId,
}

impl MachineId {
    /// Generate a machine id based on the provided string.
    ///
    /// The logic for generating the machine id is one way, but repeatable
    pub fn new(identifier: &str) -> Self {
        MachineId {
            id: RawMachineId::from_identifier(identifier),
        }
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        self.id.to_message()
    }
}

impl Default for MachineId {
    fn default() -> Self {
        Self {
            id: RawMachineId::random(),
        }
    }
}

impl From<ClientInfo> for RawClientInfo {
    fn from(value: ClientInfo) -> Self {
        RawClientInfo {
            platform_type: value.platform_type.value(),
            name: value.name,
            // Upstream wrote `value.os as i32`, the enum's declaration index
            // (Linux = 1), which `TryFrom<i32>` then rejected.
            os: value.os.into(),
            machine_id_value_bb3: value.machine_id.id.bb3,
            machine_id_value_ff2: value.machine_id.id.ff2,
            machine_id_value_3b3: value.machine_id.id.value_3b3,
        }
    }
}

impl From<RawClientInfo> for ClientInfo {
    fn from(value: RawClientInfo) -> Self {
        ClientInfo {
            platform_type: EAuthTokenPlatformType::from_i32(value.platform_type)
                .unwrap_or_default(),
            name: value.name,
            os: value.os.try_into().unwrap_or_default(),
            machine_id: MachineId {
                id: RawMachineId {
                    bb3: value.machine_id_value_bb3,
                    ff2: value.machine_id_value_ff2,
                    value_3b3: value.machine_id_value_3b3,
                },
            },
        }
    }
}

#[derive(Deserialize, Serialize)]
struct RawClientInfo {
    platform_type: i32,
    name: String,
    os: i32,
    machine_id_value_bb3: [u8; 20],
    machine_id_value_ff2: [u8; 20],
    machine_id_value_3b3: [u8; 20],
}

#[cfg(test)]
mod tests;
