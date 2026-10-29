use lbug::{Connection, Database, SystemConfig};
use std::path::PathBuf;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut core::ffi::c_void;
    fn GetModuleFileNameW(module: *mut core::ffi::c_void, file: *mut u16, length: u32) -> u32;
}

fn loaded_modules() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["lbug_shared.dll", "libssl-3-x64.dll", "libcrypto-3-x64.dll", "MSVCP140.dll", "VCRUNTIME140.dll", "VCRUNTIME140_1.dll"] {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let module = unsafe { GetModuleHandleW(wide.as_ptr()) };
        if module.is_null() { return Err(format!("expected loaded module absent: {name}").into()); }
        let mut path = vec![0u16; 32768];
        let length = unsafe { GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) } as usize;
        if length == 0 || length >= path.len() { return Err(format!("module path query failed: {name}").into()); }
        println!("MODULE {name} {}", String::from_utf16(&path[..length])?);
    }
    Ok(())
}

fn query(conn: &Connection<'_>, sql: &str) -> Result<String, Box<dyn std::error::Error>> {
    let result = conn.query(sql)?;
    let text = result.to_string();
    println!("SQL {sql}\n{text}");
    Ok(text)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mode = &args[1];
    let root = PathBuf::from(&args[2]);
    let config = SystemConfig::default().buffer_pool_size(256 * 1024 * 1024).max_num_threads(2);
    std::fs::create_dir_all(&root)?;
    if mode == "download-negative" {
        let db = Database::new(root.join("negative.ldb"), config.clone())?;
        let conn = Connection::new(&db)?;
        match conn.query("INSTALL json") {
            Ok(_) => return Err("negative control unexpectedly installed json".into()),
            Err(error) => {
                let detail = error.to_string();
                println!("EXPECTED_DOWNLOAD_FAILURE {detail}");
                if !detail.contains("Failed to download extension: json") || !detail.contains("Proxy connection failed") {
                    return Err("negative control failed for an unrelated reason".into());
                }
            }
        }
        return Ok(());
    }
    codebase_graph::db_writer::preseed_ladybug_extensions(true)?;
    println!("PRESEEDED_JSON_FTS_FROM_DEFAULT_LIBRARY");
    let dbpath = root.join("probe.ldb");
    if mode == "create" {
        let json_path = root.join("documents.json");
        std::fs::write(&json_path, "[{\"ID\":1,\"title\":\"six\",\"text\":\"Hello World\"}]")?;
        let db = Database::new(&dbpath, config.clone())?;
        let conn = Connection::new(&db)?;
        query(&conn, "LOAD json")?;
        query(&conn, "LOAD fts")?;
        loaded_modules()?;
        query(&conn, "CREATE NODE TABLE Documents (ID INT64, title STRING, text STRING, PRIMARY KEY(ID))")?;
        let copy = format!("COPY Documents FROM '{}'", json_path.to_string_lossy().replace('\\', "/").replace('\'', "''"));
        query(&conn, &copy)?;
        let rows = query(&conn, "MATCH (d:Documents) RETURN d.title, d.text")?;
        if !rows.contains("six") || !rows.contains("Hello World") { return Err("JSON COPY did not yield expected row".into()); }
        query(&conn, "CALL CREATE_FTS_INDEX('Documents', 'documents_index', ['title', 'text'], stemmer := 'english')")?;
        let result = query(&conn, "CALL QUERY_FTS_INDEX('Documents', 'documents_index', 'world') RETURN node.title")?;
        if !result.contains("six") { return Err("FTS query did not return six".into()); }
        query(&conn, "CALL SHOW_LOADED_EXTENSIONS() RETURN *")?;
        query(&conn, "CHECKPOINT")?;
        println!("PASS_JSON_COPY_FTS_CREATE_QUERY");
    } else if mode == "reopen" {
        let db = Database::new(&dbpath, config.clone())?;
        let conn = Connection::new(&db)?;
        query(&conn, "LOAD json")?;
        query(&conn, "LOAD fts")?;
        loaded_modules()?;
        let result = query(&conn, "CALL QUERY_FTS_INDEX('Documents', 'documents_index', 'world') RETURN node.title")?;
        if !result.contains("six") { return Err("reopened FTS query did not return six".into()); }
        query(&conn, "CALL SHOW_LOADED_EXTENSIONS() RETURN *")?;
        println!("PASS_FRESH_PROCESS_REOPEN_FTS_QUERY");
    } else {
        return Err("unknown mode".into());
    }
    Ok(())
}
