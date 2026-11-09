use super::*;

#[test]
fn identifier_hashes_match_steam_machine_id() {
    // Vectors from steam-machine-id, which this replaces.
    let id = MachineId::from_identifier("accountname");
    assert_eq!(
        hex::encode_upper(id.bb3),
        "6BB2445F8825BFED65E64392F0A4D549FFF7D3E1"
    );
    assert_eq!(
        hex::encode_upper(id.ff2),
        "57AD645E54976AFF3B3662E9CB335D0A24AC7D08"
    );
    assert_eq!(
        hex::encode_upper(id.value_3b3),
        "C1884025D23FB1A0DDBF125B5D9B8C0812F83390"
    );
}

#[test]
fn identifier_ids_are_repeatable_and_distinct() {
    assert_eq!(
        MachineId::from_identifier("a"),
        MachineId::from_identifier("a")
    );
    assert_ne!(
        MachineId::from_identifier("a"),
        MachineId::from_identifier("b")
    );
    assert_ne!(MachineId::random(), MachineId::random());
}

#[test]
fn message_object_layout() {
    let id = MachineId::from_identifier("accountname");
    let msg = id.to_message();
    assert_eq!(msg.len(), 155);
    assert_eq!(msg[0], 0);
    assert_eq!(&msg[1..15], b"MessageObject\0");
    for (at, key, value) in [
        (15, b"BB3\0", id.bb3),
        (61, b"FF2\0", id.ff2),
        (107, b"3B3\0", id.value_3b3),
    ] {
        assert_eq!(msg[at], 1);
        assert_eq!(&msg[at + 1..at + 5], key);
        assert_eq!(&msg[at + 5..at + 45], hex::encode_upper(value).as_bytes());
        assert_eq!(msg[at + 45], 0);
    }
    assert_eq!(&msg[153..], &[8, 8]);
}
