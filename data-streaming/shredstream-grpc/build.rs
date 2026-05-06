fn main() {
    const PROTOC_ENVAR: &str = "PROTOC";
    if std::env::var(PROTOC_ENVAR).is_err() {
        #[cfg(not(windows))]
        std::env::set_var(PROTOC_ENVAR, protobuf_src::protoc());
    }

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["protos/shredstream.proto"], &["protos"])
        .unwrap();
}
