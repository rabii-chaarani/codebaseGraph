use std::{
    env, fs,
    io::{Read, Write},
    path::PathBuf,
};
fn hex(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.get(1).map(String::as_str) == Some("start") {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        let responses = [
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25"}}"#,
            r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"graph_health"},{"name":"graph_search"}]}}"#,
            r#"{"jsonrpc":"2.0","id":3,"result":{"structuredContent":{"ok":true}}}"#,
            r#"{"jsonrpc":"2.0","id":4,"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":5,"result":{"isError":true}}"#,
        ];
        for response in responses {
            println!("{response}");
        }
        return;
    }
    let mut log = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(env::var_os("COD55_LOG").unwrap())
        .unwrap();
    writeln!(log, "CALL").unwrap();
    for arg in &args {
        writeln!(log, "ARG {}", hex(arg)).unwrap();
    }
    for key in ["HOME", "USERPROFILE", "CODEX_HOME", "OPENCLAW_HOME"] {
        writeln!(log, "ENV {key} {}", hex(&env::var(key).unwrap_or_default())).unwrap();
    }
    writeln!(
        log,
        "CWD {}",
        hex(&env::current_dir().unwrap().to_string_lossy())
    )
    .unwrap();
    let claude = env::var("COD55_CLIENT").as_deref() == Ok("claude");
    let root = if claude {
        env::current_dir().unwrap()
    } else {
        PathBuf::from(env::var_os("CODEX_HOME").unwrap())
    };
    if args.get(1).map(String::as_str) == Some("add") {
        if env::var_os("COD55_ADD_FAIL").is_some() {
            std::process::exit(23)
        }
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("native-server"), args.get(2).unwrap()).unwrap();
    } else if args.get(1).map(String::as_str) == Some("list") {
        if env::var_os("COD55_LIST_FAIL").is_some() {
            std::process::exit(25)
        }
        let contents = if claude {
            fs::read_to_string(root.join(".mcp.json")).unwrap_or_default()
        } else {
            fs::read_to_string(root.join("native-server"))
                .or_else(|_| fs::read_to_string(root.join("config.toml")))
                .unwrap_or_default()
        };
        print!("{contents}");
    } else {
        std::process::exit(24)
    }
}
