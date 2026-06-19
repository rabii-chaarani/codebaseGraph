use super::{
    install_mcp_client, install_scope, options::McpInstallOptions, supported_install_clients,
};
use crate::product_cli::format::mcp_install_help;
use serde_json::json;
use std::io::Write;

pub(in crate::product_cli) fn run_mcp_install<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = McpInstallOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", mcp_install_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let payload = if options.client == "all" {
        let results = supported_install_clients()
            .into_iter()
            .map(|client| {
                let mut client_options = options.clone();
                client_options.client = client.to_string();
                install_mcp_client(&client_options).unwrap_or_else(|error| {
                    json!({
                        "action": "failed",
                        "client": client,
                        "scope": install_scope(client, &client_options.scope),
                        "server_name": client_options.name.clone().unwrap_or_else(|| "codebase_graph".to_string()),
                        "method": serde_json::Value::Null,
                        "path": serde_json::Value::Null,
                        "command": serde_json::Value::Null,
                        "descriptor": {},
                        "entry": {},
                        "error": error,
                    })
                })
            })
            .collect::<Vec<_>>();
        json!({ "results": results })
    } else {
        install_mcp_client(&options)?
    };
    writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
