use super::*;

#[test]
fn picks_rotate_round_robin() {
    use std::net::{IpAddr, Ipv4Addr};

    let socket1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234);
    let socket2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 2345);

    let ws1 = String::from("server1:1234");
    let ws2 = String::from("server2");
    let ws3 = String::from("server3");

    let list = ServerList::new(
        vec![socket1, socket2],
        vec![ws1.clone(), ws2.clone(), ws3.clone()],
    )
    .unwrap();

    assert_eq!(vec![socket1, socket2], list.tcp_servers());
    assert_eq!(
        vec![ws1.clone(), ws2.clone(), ws3.clone()],
        list.ws_servers()
    );

    let _ = list.pick();
    let _ = list.pick_ws();
    let _ = list.pick_ws();
    let _ = list.pick_ws();

    assert_eq!(vec![socket2, socket1], list.tcp_servers());
    assert_eq!(
        vec![ws1.clone(), ws2.clone(), ws3.clone()],
        list.ws_servers()
    );
}

#[test]
fn empty_lists_are_rejected() {
    let ws = vec![String::from("server")];
    let tcp = vec!["127.0.0.1:1".parse().unwrap()];
    assert!(matches!(
        ServerList::new(vec![], ws),
        Err(ServerDiscoveryError::NoServers)
    ));
    assert!(matches!(
        ServerList::new(tcp, vec![]),
        Err(ServerDiscoveryError::NoWsServers)
    ));
}

#[test]
fn websocket_picks_are_cm_socket_urls() {
    let list = ServerList::new(
        vec!["127.0.0.1:1".parse().unwrap()],
        vec![String::from("cm1.steampowered.com:443")],
    )
    .unwrap();
    assert_eq!(list.pick_ws(), "wss://cm1.steampowered.com:443/cmsocket/");
}

#[test]
fn directory_response_parses() {
    let json = r#"{"response":{
        "serverlist":["162.254.193.6:27017","162.254.193.7:27018"],
        "serverlist_websockets":["cmp1.steamserver.net:443"],
        "result":1,"message":""}}"#;
    let response: ServerListResponse = serde_json::from_str(json).unwrap();
    let list = ServerList::try_from(response).unwrap();
    let mut tcp = list.tcp_servers();
    tcp.sort();
    assert_eq!(
        tcp,
        [
            "162.254.193.6:27017".parse::<SocketAddr>().unwrap(),
            "162.254.193.7:27018".parse().unwrap()
        ]
    );
    assert_eq!(list.ws_servers(), ["cmp1.steamserver.net:443"]);
}
