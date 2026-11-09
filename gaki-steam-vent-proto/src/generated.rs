//! Bindings `build.rs` generates from `protos/`, one module per proto file.

include!(concat!(env!("OUT_DIR"), "/generated/mod.rs"));
