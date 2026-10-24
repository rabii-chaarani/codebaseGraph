use super::*;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cohort {
    All,
    Unit,
    Generation,
    Integration,
}
impl Cohort {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "all" => Ok(Self::All),
            "unit" => Ok(Self::Unit),
            "generation" => Ok(Self::Generation),
            "integration" => Ok(Self::Integration),
            _ => Err(format!("unsupported native test cohort: {value}")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Unit => "unit",
            Self::Generation => "generation",
            Self::Integration => "integration",
        }
    }
}

fn cohort(package: &str, kind: &str, name: &str) -> Option<Cohort> {
    match kind {
        "lib" | "bin" | "example" => Some(Cohort::Unit),
        "test" if package == "codebase-graph" && name == "generation_storage" => {
            Some(Cohort::Generation)
        }
        "test" | "doctest" => Some(Cohort::Integration),
        _ => None,
    }
}

fn inventory(metadata: &Value) -> Result<Vec<Value>, String> {
    let members = metadata["workspace_members"]
        .as_array()
        .ok_or("missing workspace members")?;
    let mut inventory = Vec::new();
    for package in metadata["packages"].as_array().ok_or("missing packages")? {
        if !members.contains(&package["id"]) {
            continue;
        }
        let package_name = package["name"].as_str().ok_or("missing package name")?;
        for target in package["targets"].as_array().ok_or("missing targets")? {
            let name = target["name"].as_str().ok_or("missing target name")?;
            for kind in target["kind"].as_array().ok_or("missing target kind")? {
                let kind = kind.as_str().ok_or("invalid target kind")?;
                let mut kinds = Vec::new();
                if target["test"] == true {
                    kinds.push(kind);
                }
                if target["doctest"] == true {
                    kinds.push("doctest");
                }
                for kind in kinds {
                    if let Some(cohort) = cohort(package_name, kind, name) {
                        inventory.push(json!({"package":package_name,"target":name,
                            "kind":kind,"cohort":cohort.name()}));
                    }
                }
            }
        }
    }
    Ok(inventory)
}

fn commands(target: NativeTarget, selected: Cohort, inventory: &[Value]) -> Vec<Vec<String>> {
    let base: Vec<String> = native_test_command(target)
        .into_iter()
        .map(str::to_owned)
        .collect();
    if selected == Cohort::All {
        let mut base = base;
        base.push("--no-fail-fast".into());
        return vec![base];
    }
    let mut commands = Vec::new();
    if selected == Cohort::Unit {
        let mut command = base.clone();
        command.extend(["--lib", "--bins", "--examples"].map(str::to_owned));
        commands.push(command);
    } else {
        let mut packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for entry in inventory
            .iter()
            .filter(|e| e["cohort"] == selected.name() && e["kind"] == "test")
        {
            packages
                .entry(entry["package"].as_str().unwrap().into())
                .or_default()
                .insert(entry["target"].as_str().unwrap().into());
        }
        for (package, tests) in packages {
            let mut command: Vec<String> = base
                .iter()
                .filter(|s| *s != "--workspace")
                .cloned()
                .collect();
            command.extend(["--package".into(), package]);
            for test in tests {
                command.extend(["--test".into(), test]);
            }
            commands.push(command);
        }
        if selected == Cohort::Integration {
            let mut command = base;
            command.push("--doc".into());
            commands.push(command);
        }
    }
    commands
}

pub(super) fn run(args: Vec<String>) -> Result<(), String> {
    let options = parse_options(&args, &["--target", "--cohort"])?;
    let target = NativeTarget::parse(required_option(&options, "--target")?)?;
    let selected = Cohort::parse(options.get("--cohort").map(String::as_str).unwrap_or("all"))?;
    ensure_native_host(target)?;
    let output =
        PathBuf::from("diagnostics/tests").join(format!("{}-{}", target.as_str(), selected.name()));
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let metadata = Command::new("cargo")
        .args(["metadata", "--no-deps", "--locked", "--format-version", "1"])
        .output()
        .map_err(|e| e.to_string())?;
    if !metadata.status.success() {
        return Err(String::from_utf8_lossy(&metadata.stderr).into_owned());
    }
    let metadata: Value = serde_json::from_slice(&metadata.stdout).map_err(|e| e.to_string())?;
    let inventory = inventory(&metadata)?;
    let commands = commands(target, selected, &inventory);
    let started = Instant::now();
    let executable = capture_native_test_cli(target)?;
    let hash = sha256_file(&executable)?;
    println!(
        "native-test CLI compiler artifact: {} sha256={hash}",
        executable.display()
    );
    let mut report = json!({"source_sha":env::var("SOURCE_SHA").ok(),"target":target.as_str(),
        "cohort":selected.name(),"cli":executable,"cli_sha256":hash,"inventory":inventory,
        "commands":commands,"executions":[],"capture_seconds":started.elapsed().as_secs_f64()});
    let save = |report: &Value| {
        fs::write(
            output.join("report.json"),
            serde_json::to_vec_pretty(report).unwrap(),
        )
        .map_err(|e| e.to_string())
    };
    save(&report)?;
    let mut failures = Vec::new();
    for (index, args) in commands.iter().enumerate() {
        for (label, extra) in [
            ("inventory", vec!["--list"]),
            ("ignored", vec!["--list", "--ignored"]),
        ] {
            let listing = Command::new("cargo")
                .args(args)
                .arg("--")
                .args(extra)
                .env("CARGO_BIN_EXE_codebase-graph", &executable)
                .output()
                .map_err(|e| e.to_string())?;
            fs::write(output.join(format!("{index}-{label}.txt")), &listing.stdout)
                .map_err(|e| e.to_string())?;
            if !listing.status.success() {
                return Err(format!(
                    "native test listing failed: {}",
                    String::from_utf8_lossy(&listing.stderr)
                ));
            }
        }
        println!(
            "native test command {}: cargo {}",
            index + 1,
            args.join(" ")
        );
        let now = Instant::now();
        let result = run_command(
            Command::new("cargo")
                .args(args)
                .env("CARGO_BIN_EXE_codebase-graph", &executable),
            "native test",
        );
        let seconds = now.elapsed().as_secs_f64();
        println!(
            "native test command {} finished in {seconds:.3}s: {result:?}",
            index + 1
        );
        report["executions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"command":index,"seconds":seconds,"error":result.as_ref().err()}));
        save(&report)?;
        if let Err(error) = result {
            failures.push(error);
        }
    }
    report["total_seconds"] = json!(started.elapsed().as_secs_f64());
    report["cli_unchanged"] = json!(sha256_file(&executable).is_ok_and(|h| h == hash));
    save(&report)?;
    if report["cli_unchanged"] != true {
        return Err("captured native-test CLI changed during execution".into());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partitions_every_test_target_and_keeps_new_integration_targets() {
        let metadata = json!({"workspace_members":["root","wiki"],"packages":[
            {"id":"root","name":"codebase-graph","targets":[
                {"name":"codebase_graph","kind":["lib"],"test":true,"doctest":true},
                {"name":"codebase-graph","kind":["bin"],"test":true,"doctest":false},
                {"name":"generation_storage","kind":["test"],"test":true,"doctest":false},
                {"name":"future_test","kind":["test"],"test":true,"doctest":false}]},
            {"id":"wiki","name":"k-wiki","targets":[
                {"name":"generation_storage","kind":["test"],"test":true,"doctest":false}]}]});
        let inventory = inventory(&metadata).unwrap();
        assert_eq!(inventory.len(), 6);
        assert_eq!(
            inventory
                .iter()
                .filter(|x| x["cohort"] == "generation")
                .count(),
            1
        );
        let selected = commands(NativeTarget::MacosArm64, Cohort::Integration, &inventory);
        assert_eq!(selected.len(), 3);
        assert!(selected[0].contains(&"future_test".into()));
        assert!(!selected[0].contains(&"generation_storage".into()));
        assert!(selected[1].contains(&"generation_storage".into()));
        assert!(selected[2].contains(&"--doc".into()));
        assert!(Cohort::parse("invalid").is_err());
    }
}
