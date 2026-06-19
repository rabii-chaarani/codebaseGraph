use super::{
    attach_install_verification, build_mcp_descriptor, copilot_studio_metadata,
    default_client_config_path, executable_in_path, install_scope, native_client_command,
    render_client_config, subprocess_error, write_text_atomic, McpInstallOptions,
    NativeMcpDescriptor,
};
use serde_json::json;
use std::{fs, process::Command};

pub(in crate::product_cli) fn install_mcp_client(
    options: &McpInstallOptions,
) -> Result<serde_json::Value, String> {
    let descriptor = build_mcp_descriptor(options)?;
    if options.client == "copilot-studio" || options.client == "microsoft-copilot" {
        let metadata = copilot_studio_metadata(&descriptor);
        let payload = json!({
            "action": if options.dry_run { "dry_run" } else { "reported" },
            "client": options.client,
            "scope": options.scope,
            "server_name": descriptor.name,
            "method": "manual_metadata",
            "path": serde_json::Value::Null,
            "command": serde_json::Value::Null,
            "descriptor": descriptor.as_json(),
            "entry": metadata["stdio"].clone(),
            "payload": metadata,
        });
        return attach_install_verification(payload, &descriptor, options);
    }
    let native_command = native_client_command(&options.client, &descriptor, &options.scope);
    let native_available = native_command
        .as_ref()
        .and_then(|command| command.first())
        .is_some_and(|executable| executable_in_path(executable));
    if options.dry_run && options.client_config_path.is_none() && native_available {
        return attach_install_verification(
            json!({
                "action": "dry_run",
                "client": options.client,
                "scope": install_scope(&options.client, &options.scope),
                "server_name": descriptor.name,
                "method": "native_cli",
                "path": serde_json::Value::Null,
                "command": native_command,
                "descriptor": descriptor.as_json(),
                "entry": descriptor.stdio_entry(false, false),
            }),
            &descriptor,
            options,
        );
    }
    if !options.dry_run && options.client_config_path.is_none() && native_available {
        let Some(command) = native_command.clone() else {
            return file_adapter_result(options, &descriptor, native_command, None);
        };
        let completed = Command::new(&command[0])
            .args(&command[1..])
            .output()
            .map_err(|error| format!("failed to run native client installer: {error}"))?;
        if completed.status.success() {
            return attach_install_verification(
                json!({
                    "action": "updated",
                    "client": options.client,
                    "scope": install_scope(&options.client, &options.scope),
                    "server_name": descriptor.name,
                    "method": "native_cli",
                    "path": serde_json::Value::Null,
                    "command": command,
                    "descriptor": descriptor.as_json(),
                    "entry": descriptor.stdio_entry(false, false),
                }),
                &descriptor,
                options,
            );
        }
        let error = subprocess_error(&completed);
        return file_adapter_result(options, &descriptor, Some(command), Some(error));
    }
    let native_error = native_command.as_ref().and_then(|command| {
        command.first().and_then(|executable| {
            if executable_in_path(executable) {
                None
            } else {
                Some(format!("{executable} executable not found"))
            }
        })
    });
    file_adapter_result(options, &descriptor, native_command, native_error)
}

pub(in crate::product_cli) fn file_adapter_result(
    options: &McpInstallOptions,
    descriptor: &NativeMcpDescriptor,
    native_command: Option<Vec<String>>,
    native_error: Option<String>,
) -> Result<serde_json::Value, String> {
    let path = options.client_config_path.clone().unwrap_or_else(|| {
        default_client_config_path(
            &options.client,
            &install_scope(&options.client, &options.scope),
            descriptor,
        )
    });
    let existing = fs::read_to_string(&path).ok();
    let rendered = render_client_config(
        &options.client,
        &install_scope(&options.client, &options.scope),
        existing.as_deref(),
        descriptor,
    )?;
    let action = if options.dry_run {
        "dry_run".to_string()
    } else {
        rendered.action.clone()
    };
    if !options.dry_run {
        write_text_atomic(&path, &rendered.text)?;
    }
    let mut payload = json!({
        "action": action,
        "client": options.client,
        "scope": install_scope(&options.client, &options.scope),
        "server_name": descriptor.name,
        "method": "file_adapter",
        "path": path.to_string_lossy(),
        "command": serde_json::Value::Null,
        "descriptor": descriptor.as_json(),
        "entry": rendered.entry,
        "patch": rendered.patch,
        "payload": rendered.payload,
    });
    if let Some(command) = native_command {
        payload["native_command"] = json!(command);
    }
    if let Some(error) = native_error {
        payload["native_error"] = json!(error);
    }
    attach_install_verification(payload, descriptor, options)
}
