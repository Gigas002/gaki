use super::*;

#[test]
fn os_wire_values() {
    for (os, value) in [
        (Os::Web, -700),
        (Os::Linux, -203),
        (Os::MacOs, -102),
        (Os::Windows, 0),
    ] {
        assert_eq!(i32::from(os), value);
        assert_eq!(Os::try_from(value), Ok(os));
    }
    assert!(Os::try_from(1).is_err());
}

#[test]
fn client_info_round_trips_through_serde() {
    let mut info = ClientInfo::new("gaki".into());
    info.os = Os::Linux;
    info.machine_id = MachineId::new("account");
    let json = serde_json::to_string(&info).unwrap();
    let back: ClientInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, "gaki");
    // Upstream serialised the enum index (1) and read it back as Windows.
    assert_eq!(back.os, Os::Linux);
    assert_eq!(back.machine_id, info.machine_id);
    assert_eq!(back.platform_type, info.platform_type);
}

#[test]
fn machine_id_is_derived_from_its_identifier() {
    assert_eq!(MachineId::new("a"), MachineId::new("a"));
    assert_ne!(MachineId::new("a"), MachineId::new("b"));
    assert_eq!(MachineId::new("a").encode().len(), 155);
}
