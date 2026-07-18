//! Integration tests for the dprint Oxfmt process plugin.

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::Command;
    use std::process::Stdio;
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

            let file_path = PathBuf::from("/tmp/dprint-plugin-oxfmt-example.ts");
            let source_text = "const value=\"hello\"\n";
            let options = serde_json::json!({ "singleQuote": true });
            let expected = direct_oxfmt(&file_path, source_text, &options);
            let expected_code = expected["code"]
                .as_str()
                .expect("Oxfmt oracle should return code");

            let result = communicator
                .format_text(ProcessPluginCommunicatorFormatRequest {
                    file_path,
                    file_bytes: source_text.as_bytes().to_vec(),
                    range: None,
                    config_id,
                    override_config: ConfigKeyMap::new(),
                    on_host_format: Rc::new(|_request| Box::pin(async { Ok(None) })),
                    token: Arc::new(NullCancellationToken),
                })
                .await
                .expect("format request should succeed");

            assert_eq!(result, Some(expected_code.as_bytes().to_vec()));
            communicator.shutdown().await;
        });
    }

    fn direct_oxfmt(
        file_path: &std::path::Path,
        source_text: &str,
        options: &serde_json::Value,
    ) -> serde_json::Value {
        let request = serde_json::json!({
            "fileName": file_path,
            "sourceText": source_text,
            "options": options,
        });
        let mut child = Command::new(node_program())
            .arg(oracle_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Oxfmt oracle should start");
        child
            .stdin
            .take()
            .expect("oracle stdin should be piped")
            .write_all(request.to_string().as_bytes())
            .expect("oracle request should be writable");
        let output = child.wait_with_output().expect("Oxfmt oracle should exit");
        assert!(
            output.status.success(),
            "Oxfmt oracle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("Oxfmt oracle output should be JSON")
    }

    fn node_program() -> PathBuf {
        std::env::var_os("DPRINT_OXFMT_NODE").map_or_else(|| PathBuf::from("node"), PathBuf::from)
    }

    fn oracle_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/dist/oracle.js")
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
