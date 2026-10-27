fn main() {
    use std::{fs, path::{Component, PathBuf}};
    use codebase_graph::db_writer::{connect_ladybug_database, open_ladybug_database};
    use fs2::FileExt;
    let root = PathBuf::from(std::env::var_os("PROBE_ROOT").unwrap());
    let native_parent = root.join("tilde-native");
    let cwd = root.join("tilde-probe-cwd/deep");
    fs::create_dir_all(&native_parent).unwrap();
    fs::create_dir_all(&cwd).unwrap();
    std::env::set_current_dir(&cwd).unwrap();
    let native_path = native_parent.join("graph.lbug");
    drop(open_ladybug_database(&native_path, false).unwrap());
    let actual_user_home = PathBuf::from(std::env::var_os("HOME").unwrap());
    assert!(actual_user_home.is_absolute());
    let mut addressed = PathBuf::from("~");
    for component in actual_user_home.components() {
        if matches!(component, Component::Normal(_)) { addressed.push(".."); }
    }
    addressed.push(native_path.strip_prefix("/").unwrap());
    let guarded = open_ladybug_database(&addressed, true).unwrap();
    let connection = connect_ladybug_database(&guarded).unwrap();
    connection.query("RETURN 1").unwrap();
    let family = fs::OpenOptions::new().read(true).write(true)
        .open(native_parent.join(".database-use-family.lock")).unwrap();
    let actual_exclusive = family.try_lock_exclusive();
    println!("addressed={}", addressed.display());
    println!("guarded_parent={}", fs::canonicalize(addressed.parent().unwrap()).unwrap().display());
    println!("native_parent={}", fs::canonicalize(&native_parent).unwrap().display());
    println!("addressed_database_exists={}", addressed.exists());
    println!("native_database_exists={}", native_path.exists());
    println!("actual_native_family_exclusive_available={}", actual_exclusive.is_ok());
    assert!(actual_exclusive.is_ok(), "guard unexpectedly covers actual native parent");
    assert!(!addressed.exists());
    connection.query("RETURN 2").unwrap();
    drop(connection);
    drop(guarded);
    drop(family);
}
