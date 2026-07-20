//! Integration tests for the dprint Oxfmt process plugin.

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};
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
    #[ignore = "requires the runtime and plugin binary to be built"]
    fn formats_core_fixtures_through_real_dprint_and_node_processes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed creating Tokio runtime");

        runtime.block_on(async {
            let plugin = plugin_binary_path();
            assert_file_exists(&plugin, "plugin binary");
            assert_file_exists(&worker_entry_path(), "Node worker");

            let communicator = ProcessPluginCommunicator::new(&plugin, |message| {
                eprintln!("plugin stderr: {message}");
            })
            .await
            .expect("plugin process should start");

            format_core_fixtures(&communicator).await;
            verify_syntax_error(&communicator).await;

            communicator.shutdown().await;
        });
    }

    async fn format_core_fixtures(communicator: &ProcessPluginCommunicator) {
        let cases = [
            (
                "typescript",
                "typescript.input.ts",
                "typescript.expected.ts",
                ConfigKeyMap::new(),
                false,
            ),
            (
                "single-quote",
                "single-quote.input.ts",
                "single-quote.expected.ts",
                single_quote_config(),
                false,
            ),
            (
                "already-formatted",
                "already-formatted.input.ts",
                "already-formatted.expected.ts",
                ConfigKeyMap::new(),
                true,
            ),
        ];

        for (index, (name, input_name, expected_name, config, unchanged)) in
            cases.into_iter().enumerate()
        {
            let config_id = FormatConfigId::from_raw(
                u32::try_from(index + 1).expect("fixture index should fit in a config id"),
            );
            communicator
                .register_config(config_id, &GlobalConfiguration::default(), &config)
                .await
                .unwrap_or_else(|error| panic!("{name} config should register: {error}"));

            let input_path = basic_fixture_path(input_name);
            let input = std::fs::read(&input_path)
                .unwrap_or_else(|error| panic!("{name} input should be readable: {error}"));
            let expected_path = basic_fixture_path(expected_name);
            let expected = std::fs::read(&expected_path)
                .unwrap_or_else(|error| panic!("{name} expected should be readable: {error}"));
            let result = communicator
                .format_text(ProcessPluginCommunicatorFormatRequest {
                    file_path: input_path,
                    file_bytes: input,
                    range: None,
                    config_id,
                    override_config: ConfigKeyMap::new(),
                    on_host_format: Rc::new(|_request| Box::pin(async { Ok(None) })),
                    token: Arc::new(NullCancellationToken),
                })
                .await
                .unwrap_or_else(|error| panic!("{name} format should succeed: {error}"));

            if unchanged {
                assert_eq!(result, None, "{name} should report no change");
            } else {
                // Git may check out fixtures with CRLF on Windows while Oxfmt emits LF.
                assert_eq!(
                    result.map(|bytes| normalize_line_endings(&bytes)),
                    Some(normalize_line_endings(&expected)),
                    "{name} output should match Oxfmt"
                );
            }
        }
    }

    fn normalize_line_endings(bytes: &[u8]) -> Vec<u8> {
        let mut normalized = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'\r' {
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
                normalized.push(b'\n');
            } else {
                normalized.push(bytes[index]);
            }
            index += 1;
        }
        normalized
    }

    #[test]
    fn normalizes_all_supported_line_endings_to_lf() {
        assert_eq!(
            normalize_line_endings(b"first\r\nsecond\rthird\nfourth"),
            b"first\nsecond\nthird\nfourth"
        );
    }

    async fn verify_syntax_error(communicator: &ProcessPluginCommunicator) {
        let config_id = FormatConfigId::from_raw(4);
        communicator
            .register_config(
                config_id,
                &GlobalConfiguration::default(),
                &ConfigKeyMap::new(),
            )
            .await
            .expect("error config should register");
        let file_path = error_fixture_path("syntax-error.input.ts");
        let error = communicator
            .format_text(ProcessPluginCommunicatorFormatRequest {
                file_path: file_path.clone(),
                file_bytes: std::fs::read(&file_path).expect("error input should be readable"),
                range: None,
                config_id,
                override_config: ConfigKeyMap::new(),
                on_host_format: Rc::new(|_request| Box::pin(async { Ok(None) })),
                token: Arc::new(NullCancellationToken),
            })
            .await
            .expect_err("syntax errors should fail formatting")
            .to_string();
        assert!(error.contains("syntax-error.input.ts"));
        assert!(error.contains("Unexpected token"));
    }

    #[test]
    #[ignore = "requires the runtime and plugin binary to be built"]
    fn formats_through_the_real_dprint_and_node_processes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed creating Tokio runtime");

        runtime.block_on(async {
            let plugin = plugin_binary_path();
            assert_file_exists(&plugin, "plugin binary");
            assert_file_exists(&worker_entry_path(), "Node worker");

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

    fn single_quote_config() -> ConfigKeyMap {
        let mut config = ConfigKeyMap::new();
        config.insert("singleQuote".to_owned(), ConfigKeyValue::Bool(true));
        config
    }

    fn basic_fixture_path(name: &str) -> PathBuf {
        fixture_path("basic", name)
    }

    fn error_fixture_path(name: &str) -> PathBuf {
        fixture_path("errors", name)
    }

    fn fixture_path(category: &str, name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(category)
            .join(name)
    }

    fn worker_entry_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/dist/worker.js")
    }

    fn direct_oxfmt(
        file_path: &Path,
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
        if let Some(path) = std::env::var_os("DPRINT_OXFMT_PLUGIN") {
            return PathBuf::from(path);
        }

        let current_exe = std::env::current_exe().expect("test executable path should exist");
        let debug_dir = current_exe
            .parent()
            .and_then(Path::parent)
            .expect("test executable should be under target/debug/deps");
        let mut plugin = debug_dir.join("dprint-plugin-oxfmt");
        if cfg!(windows) {
            plugin.set_extension("exe");
        }
        plugin
    }

    fn assert_file_exists(path: &Path, description: &str) {
        assert!(
            path.is_file(),
            "{description} not found at {}. Build it first with `pnpm --dir runtime build` and `cargo build -p dprint-plugin-oxfmt`.",
            path.display()
        );
    }
}
