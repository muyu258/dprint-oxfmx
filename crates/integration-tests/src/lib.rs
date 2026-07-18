//! Integration tests for the dprint Oxfmt process plugin.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Arc;

    use dprint_core::configuration::ConfigKeyMap;
    use dprint_core::configuration::ConfigKeyValue;
    use dprint_core::configuration::GlobalConfiguration;
    use dprint_core::plugins::FormatConfigId;
    use dprint_core::plugins::NullCancellationToken;
    use dprint_core::plugins::process::ProcessPluginCommunicator;
    use dprint_core::plugins::process::ProcessPluginCommunicatorFormatRequest;

    #[test]
    #[ignore = "requires the plugin binary to be built"]
    fn formats_through_the_real_dprint_and_node_processes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed creating test runtime");

        runtime.block_on(async {
            let plugin = plugin_binary_path();
            let communicator = ProcessPluginCommunicator::new(&plugin, |message| {
                eprintln!("plugin stderr: {message}");
            })
            .await
            .expect("plugin process should start");

            let config_id = FormatConfigId::from_raw(1);
            let mut plugin_config = ConfigKeyMap::new();
            plugin_config.insert("singleQuote".to_owned(), ConfigKeyValue::Bool(true));
            communicator
                .register_config(config_id, &GlobalConfiguration::default(), &plugin_config)
                .await
                .expect("plugin config should register");

            let result = communicator
                .format_text(ProcessPluginCommunicatorFormatRequest {
                    file_path: PathBuf::from("/tmp/dprint-plugin-oxfmt-example.ts"),
                    file_bytes: b"const value=\"hello\"\n".to_vec(),
                    range: None,
                    config_id,
                    override_config: ConfigKeyMap::new(),
                    on_host_format: Rc::new(|_request| Box::pin(async { Ok(None) })),
                    token: Arc::new(NullCancellationToken),
                })
                .await
                .expect("format request should succeed");

            assert_eq!(result, Some(b"const value = 'hello';\n".to_vec()));
            communicator.shutdown().await;
        });
    }

    fn plugin_binary_path() -> PathBuf {
        let current_exe = std::env::current_exe().expect("test executable path should exist");
        let debug_dir = current_exe
            .parent()
            .and_then(std::path::Path::parent)
            .expect("test executable should be under target/debug/deps");
        let mut plugin = debug_dir.join("dprint-plugin-oxfmt");
        if cfg!(windows) {
            plugin.set_extension("exe");
        }
        plugin
    }
}
