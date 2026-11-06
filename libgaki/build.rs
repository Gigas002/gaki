// Generate protobuf messages that steam-vent-proto does not ship, with pure-Rust
// rust-protobuf codegen — no protoc required. Each proto lands in its own
// `$OUT_DIR/<name>/` and is `include!`d from the matching src/steam_client module.
// A proto is only generated when the feature that uses it is enabled.
fn main() {
    #[cfg(feature = "authentication")]
    for (feature, proto, out) in [
        (
            "COLLECTIONS",
            "proto/service_cloudconfigstore.proto",
            "cloudconfig",
        ),
        ("STORE", "proto/service_storequery.proto", "storequery"),
        ("STORE", "proto/service_wishlist.proto", "wishlist"),
    ] {
        println!("cargo:rerun-if-changed={proto}");
        if std::env::var_os(format!("CARGO_FEATURE_{feature}")).is_none() {
            continue;
        }
        protobuf_codegen::Codegen::new()
            .pure()
            .include("proto")
            .input(proto)
            .cargo_out_dir(out)
            .run_from_script();
    }
}
