use super::{
    options::McpServeOptions,
    protocol::{handle_mcp_message, parse_mcp_payload, rpc_error, McpSession},
    refresh::start_auto_refresh,
};
use serde_json::json;
use std::io::{BufRead, Write};

pub(in crate::cli) fn serve_mcp_stdio<R: BufRead, W: Write>(
    options: &McpServeOptions,
    mut input: R,
    output: &mut W,
) -> Result<(), String> {
    let mut options = options.clone();
    options.refresh = Some(start_auto_refresh(&options));
    let mut session = McpSession::default();
    while let Some(message) = read_mcp_message(&mut input, output)? {
        if let Some(response) = handle_mcp_message(message, &mut session, &options) {
            write_mcp_message(output, &response)?;
        }
    }
    Ok(())
}

pub(in crate::cli) fn read_mcp_message<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
) -> Result<Option<serde_json::Value>, String> {
    let mut line = String::new();
    let bytes = input
        .read_line(&mut line)
        .map_err(|error| format!("failed to read MCP frame: {error}"))?;
    if bytes == 0 {
        return Ok(None);
    }
    if line.to_ascii_lowercase().starts_with("content-length:") {
        let length = match line
            .split_once(':')
            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        {
            Some(length) => length,
            None => {
                write_mcp_message(
                    output,
                    &rpc_error(
                        serde_json::Value::Null,
                        -32700,
                        "Invalid JSON-RPC payload: Content-Length must be an integer",
                    ),
                )?;
                return Ok(None);
            }
        };
        loop {
            line.clear();
            let bytes = input
                .read_line(&mut line)
                .map_err(|error| format!("failed to read MCP headers: {error}"))?;
            if bytes == 0 || line == "\n" || line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0_u8; length];
        input.read_exact(&mut body).map_err(|error| {
            format!("Body ended before Content-Length bytes were read: {error}")
        })?;
        return parse_mcp_payload(&body).map(Some).or_else(|error| {
            log_mcp_stdio_parse_error(&error);
            write_mcp_message(
                output,
                &rpc_error(
                    serde_json::Value::Null,
                    -32700,
                    &format!("Invalid JSON-RPC payload: {error}"),
                ),
            )?;
            Ok(None)
        });
    }
    parse_mcp_payload(line.as_bytes())
        .map(Some)
        .or_else(|error| {
            log_mcp_stdio_parse_error(&error);
            write_mcp_message(
                output,
                &rpc_error(
                    serde_json::Value::Null,
                    -32700,
                    &format!("Invalid JSON-RPC payload: {error}"),
                ),
            )?;
            Ok(None)
        })
}

pub(in crate::cli) fn log_mcp_stdio_parse_error(error: &str) {
    eprintln!(
        "{}",
        json!({
            "event": "mcp.stdio_parse_error",
            "message": error,
        })
    );
}

pub(in crate::cli) fn write_mcp_message<W: Write>(
    output: &mut W,
    message: &serde_json::Value,
) -> Result<(), String> {
    let body = serde_json::to_string(message).map_err(|error| error.to_string())?;
    writeln!(output, "{body}").map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}
