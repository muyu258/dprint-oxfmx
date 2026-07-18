mod configuration;
mod handler;
#[cfg(test)]
mod mock_worker;

use dprint_core::plugins::FormatError;
use dprint_core::plugins::process::get_parent_process_id_from_cli_args;
use dprint_core::plugins::process::handle_process_stdio_messages;
use dprint_core::plugins::process::start_parent_process_checker_task;
use handler::OxfmtPluginHandler;

fn main() -> Result<(), FormatError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("failed creating Tokio runtime");

    runtime.block_on(async {
        if let Some(parent_process_id) = get_parent_process_id_from_cli_args() {
            start_parent_process_checker_task(parent_process_id);
        }

        handle_process_stdio_messages(OxfmtPluginHandler::new()?).await
    })
}
