// Based on steam-vent-proto (MIT); see README.md and LICENSE-MIT.
// Generate the Steam protobuf bindings from `protos/` at build time, with
// pure-Rust rust-protobuf codegen (no protoc), like libgaki's own protos.
//
// Port of the upstream steam-vent-proto-build generator: on top of the plain
// rust-protobuf output, every file gets
// - `RpcMessage` (and, when an `EMsg`-style enum names it, `RpcMessageWithKind`)
//   impls for its messages,
// - an `RpcService` marker struct per service and an `RpcMethod` impl per
//   method request, so `Connection::service_method` knows the method name and
//   response type,
// - `MsgKindEnum` impls for its `E*Msg`/`E*Messages` enums.
//
// Generating here instead of vendoring the output keeps the generated code in
// step with the `protobuf` runtime the workspace pins.

use protobuf::reflect::{FileDescriptor, MessageDescriptor, ServiceDescriptor};
use protobuf::{Message, SpecialFields, UnknownValueRef};
use protobuf_codegen::{Codegen, Customize, CustomizeCallback};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const PROTOS: &str = "protos";

fn main() {
    println!("cargo:rerun-if-changed={PROTOS}");
    println!("cargo:rerun-if-changed=build.rs");

    let mut protos: Vec<PathBuf> = std::fs::read_dir(PROTOS)
        .expect("read protos/")
        .map(|entry| entry.expect("read protos/ entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "proto"))
        .collect();
    protos.sort();

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("generated");
    // Stale modules from an earlier proto set would otherwise linger.
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("create generated dir");

    let generator = ServiceGenerator::new(get_kinds(Path::new(PROTOS), &protos));
    let files = Rc::clone(&generator.files);

    Codegen::new()
        .pure()
        .out_dir(&out_dir)
        .include(PROTOS)
        .inputs(&protos)
        .customize(Customize::default().lite_runtime(true).gen_mod_rs(true))
        .customize_callback(generator)
        .run_from_script();

    for (proto, services) in files.borrow().iter() {
        if services.is_empty() {
            continue;
        }
        let path = out_dir.join(format!("{}.rs", proto_path_to_rust_mod(proto)));
        let mut code = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        code.push('\n');
        services.gen_into(&mut code);
        std::fs::write(&path, code).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
}

#[derive(Debug, Clone)]
struct ServiceMethod {
    name: String,
    service_name: String,
    description: Option<String>,
    request: String,
    response: String,
}

impl ServiceMethod {
    fn gen_into(&self, out: &mut String) {
        let response = if self.response == "NoResponse" {
            "()"
        } else {
            &self.response
        };
        let _ = writeln!(
            out,
            "impl crate::RpcMethod for {request} {{\n    \
             const METHOD_NAME: &'static str = \"{service}.{name}#1\";\n    \
             type Response = {response};\n}}",
            request = self.request,
            service = self.service_name,
            name = self.name,
        );
    }
}

#[derive(Debug)]
struct Service {
    name: String,
    description: Option<String>,
    methods: Vec<ServiceMethod>,
}

impl Service {
    fn gen_into(&self, out: &mut String) {
        let _ = writeln!(
            out,
            "#[doc = {desc:?}]\nstruct {name} {{}}\n\
             impl crate::RpcService for {name} {{\n    \
             const SERVICE_NAME: &'static str = \"{name}\";\n}}",
            desc = self.description.as_deref().unwrap_or_default(),
            name = self.name,
        );
    }
}

impl From<ServiceDescriptor> for Service {
    fn from(value: ServiceDescriptor) -> Self {
        let name = value.proto().name().to_string();
        let methods = value
            .methods()
            .map(|method| ServiceMethod {
                name: method.proto().name().to_string(),
                service_name: name.clone(),
                description: get_description(method.proto().options.special_fields()),
                request: method.input_type().full_name().to_string(),
                response: method.output_type().full_name().to_string(),
            })
            .collect();
        Service {
            description: get_description(value.proto().options.special_fields()),
            name,
            methods,
        }
    }
}

struct ServiceMessage {
    name: String,
    kind: Option<Kind>,
}

impl ServiceMessage {
    fn gen_into(&self, out: &mut String) {
        let _ = writeln!(
            out,
            "impl crate::RpcMessage for {name} {{\n    \
             fn parse(reader: &mut dyn ::std::io::Read) -> ::protobuf::Result<Self> {{\n        \
             <Self as ::protobuf::Message>::parse_from_reader(reader)\n    }}\n    \
             fn write(&self, writer: &mut dyn ::std::io::Write) -> ::protobuf::Result<()> {{\n        \
             ::protobuf::Message::write_to_writer(self, writer)\n    }}\n    \
             fn encode_size(&self) -> usize {{\n        \
             ::protobuf::Message::compute_size(self) as usize\n    }}\n}}",
            name = self.name,
        );
        if let Some(kind) = &self.kind {
            let _ = writeln!(
                out,
                "impl crate::RpcMessageWithKind for {name} {{\n    \
                 type KindEnum = {enum_path};\n    \
                 const KIND: Self::KindEnum = {enum_path}::{variant};\n}}",
                name = self.name,
                enum_path = kind.enum_path(),
                variant = kind.variant,
            );
        }
    }
}

struct FileServices {
    services: Vec<Service>,
    imports: Vec<String>,
    messages: Vec<ServiceMessage>,
    kind_enums: Vec<String>,
}

impl FileServices {
    fn is_empty(&self) -> bool {
        // Imports alone generate nothing worth importing for.
        self.services.is_empty() && self.messages.is_empty() && self.kind_enums.is_empty()
    }

    fn gen_into(&self, out: &mut String) {
        for import in &self.imports {
            let _ = writeln!(
                out,
                "#[allow(unused_imports)]\nuse crate::{}::*;",
                proto_path_to_rust_mod(import)
            );
        }
        for message in &self.messages {
            message.gen_into(out);
        }
        for service in &self.services {
            service.gen_into(out);
        }
        // Two services can share a request type; emit each request's impl once.
        let mut methods: BTreeMap<&str, &ServiceMethod> = BTreeMap::new();
        for method in self.services.iter().flat_map(|s| &s.methods) {
            methods.entry(&method.request).or_insert(method);
        }
        for method in methods.values() {
            method.gen_into(out);
        }
        for kind_enum in &self.kind_enums {
            let _ = writeln!(out, "impl crate::MsgKindEnum for {kind_enum} {{}}");
        }
    }
}

struct ServiceGenerator {
    files: Rc<RefCell<BTreeMap<String, FileServices>>>,
    descriptions: RefCell<BTreeMap<String, String>>,
    kinds: Vec<Kind>,
}

impl ServiceGenerator {
    fn new(kinds: Vec<Kind>) -> Self {
        Self {
            files: Rc::default(),
            descriptions: RefCell::default(),
            kinds,
        }
    }

    fn find_kind(&self, message_type: &str, file_name: Option<&str>) -> Option<Kind> {
        self.kinds
            .iter()
            .find(|kind| kind.matches(message_type, file_name))
            .cloned()
    }
}

/// Steam's `(description)` service/method option. It isn't declared in any
/// imported proto, so it only survives as an unknown length-delimited field.
fn get_description(fields: &SpecialFields) -> Option<String> {
    fields
        .unknown_fields()
        .iter()
        .find_map(|(_, value)| match value {
            UnknownValueRef::LengthDelimited(bytes) => String::from_utf8(bytes.to_vec()).ok(),
            _ => None,
        })
}

fn is_kind_enum(name: &str) -> bool {
    name.starts_with('E') && (name.ends_with("Msg") || name.ends_with("Messages"))
}

impl CustomizeCallback for ServiceGenerator {
    fn file(&self, file: &FileDescriptor) -> Customize {
        let services: Vec<Service> = file.services().map(Service::from).collect();
        let imports = file
            .deps()
            .iter()
            .map(|dep| dep.name().to_string())
            .filter(|import| !import.starts_with("google"))
            .collect();
        let messages = file
            .messages()
            .map(|msg| ServiceMessage {
                name: msg.name().to_string(),
                kind: self
                    .find_kind(msg.name(), Some(file.name()))
                    .or_else(|| self.find_kind(msg.name(), None)),
            })
            .collect();
        let kind_enums = file
            .enums()
            .map(|e| e.name().to_string())
            .filter(|name| is_kind_enum(name))
            .collect();

        let mut descriptions = self.descriptions.borrow_mut();
        for method in services.iter().flat_map(|s| &s.methods) {
            if let Some(description) = &method.description {
                descriptions.insert(method.request.clone(), description.clone());
            }
        }

        self.files.borrow_mut().insert(
            file.name().to_string(),
            FileServices {
                services,
                imports,
                messages,
                kind_enums,
            },
        );
        Customize::default()
    }

    fn message(&self, message: &MessageDescriptor) -> Customize {
        match self.descriptions.borrow().get(message.name()) {
            Some(description) => Customize::default().before(&format!("#[doc = {description:?}]")),
            None => Customize::default(),
        }
    }
}

/// Every variant of every `E*Msg`/`E*Messages` enum, longest enum prefix first
/// so the most specific enum wins a match.
fn get_kinds(include: &Path, protos: &[PathBuf]) -> Vec<Kind> {
    let parsed = protobuf_parse::Parser::new()
        .pure()
        .include(include)
        .inputs(protos)
        .parse_and_typecheck()
        .expect("parse protos")
        .file_descriptors;
    let mut kinds: Vec<Kind> = parsed
        .iter()
        .flat_map(|file| {
            let mod_name = proto_path_to_rust_mod(file.name());
            file.enum_type
                .iter()
                .filter(|e| is_kind_enum(e.name()))
                .flat_map(move |e| {
                    let mod_name = mod_name.clone();
                    e.value
                        .iter()
                        .map(move |v| Kind::new(&mod_name, e.name(), v.name()))
                })
        })
        .collect();
    kinds.sort_by_key(|kind| std::cmp::Reverse(kind.enum_prefix.len()));
    kinds
}

/// One `EMsg`-style enum variant, matched to the message struct it tags.
#[derive(Debug, Clone)]
struct Kind {
    mod_name: String,
    enum_name: String,
    enum_prefix: String,
    variant_prefix: String,
    variant_prefix_alt: String,
    variant: String,
    is_gc: bool,
    struct_name_prefix_alt_len: usize,
}

impl Kind {
    fn new(mod_name: &str, enum_name: &str, variant: &str) -> Self {
        // `EGCItemMsg` -> "GCI" -> "GC"; `EMsg` -> "" (the trailing capital
        // starts the `Msg` suffix, not the prefix).
        let mut prefix: String = enum_name
            .chars()
            .skip(1)
            .take_while(char::is_ascii_uppercase)
            .collect();
        prefix.pop();
        let variant_prefix = format!("k_EMsg{prefix}");
        Kind {
            is_gc: variant_prefix.contains("GC"),
            mod_name: mod_name.to_string(),
            enum_name: enum_name.to_string(),
            enum_prefix: prefix.to_ascii_lowercase(),
            variant_prefix,
            variant_prefix_alt: format!("k_E{prefix}Msg_"),
            variant: variant.to_string(),
            struct_name_prefix_alt_len: prefix.len(),
        }
    }

    fn matches(&self, struct_name: &str, file_name: Option<&str>) -> bool {
        let struct_name = struct_name.strip_prefix('C').unwrap_or(struct_name);
        let struct_name = struct_name.strip_prefix("Msg").unwrap_or(struct_name);

        let Some(stripped) = self
            .variant
            .strip_prefix(&self.variant_prefix)
            .or_else(|| self.variant.strip_prefix(&self.variant_prefix_alt))
            .or_else(|| self.variant.strip_prefix("k_EMsg"))
        else {
            return false;
        };
        if let Some(file_name) = file_name
            && !self.is_gc
            && !self.enum_prefix.is_empty()
            && !file_name.contains(&self.enum_prefix)
            && !file_name.replace('_', "").contains(&self.enum_prefix)
        {
            return false;
        }
        struct_name.eq_ignore_ascii_case(stripped)
            || (self.is_gc
                && stripped
                    .strip_prefix("GC")
                    .unwrap_or_default()
                    .eq_ignore_ascii_case(struct_name))
            || struct_name
                .get(self.struct_name_prefix_alt_len..)
                .unwrap_or_default()
                .eq_ignore_ascii_case(stripped)
    }

    fn enum_path(&self) -> String {
        format!("crate::{}::{}", self.mod_name, self.enum_name)
    }
}

/// `steammessages_auth.steamclient.proto` -> `steammessages_auth_steamclient`,
/// the module name rust-protobuf gives the file.
fn proto_path_to_rust_mod(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.strip_suffix(".proto").unwrap_or(file);
    stem.chars()
        .enumerate()
        .map(|(i, c)| {
            if c.is_ascii_alphabetic() || c == '_' || (i > 0 && c.is_ascii_digit()) {
                c
            } else {
                '_'
            }
        })
        .collect()
}
