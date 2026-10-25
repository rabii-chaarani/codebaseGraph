use super::*;

// Keep Cargo plus the forwarded harness arguments well below Windows' 32 KiB
// command-line ceiling, including Cargo's executable/path/environment options.
const ARGUMENT_UNITS: usize = 8 * 1024;

#[derive(Clone, Copy)]
pub(super) struct Shard {
    pub(super) part: usize,
    pub(super) total: usize,
}
impl Shard {
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        let invalid = || format!("invalid test shard {value}; expected I/N with 1 <= I <= N <= 16");
        let (part, total) = value.split_once('/').ok_or_else(invalid)?;
        let part: usize = part.parse().map_err(|_| invalid())?;
        let total: usize = total.parse().map_err(|_| invalid())?;
        if part == 0 || part > total || total > 16 {
            return Err(invalid());
        }
        Ok(Self { part, total })
    }
    pub(super) fn commands(
        self,
        base: &[String],
        listing: &str,
    ) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
        let all: BTreeSet<_> = listing
            .lines()
            .filter_map(|l| l.strip_suffix(": test"))
            .map(str::to_owned)
            .collect();
        // Spread contiguous module families, including the expensive collector
        // and lifecycle fixtures, evenly across the required serial runners.
        let names: Vec<_> = all
            .into_iter()
            .enumerate()
            .filter_map(|(index, name)| (index % self.total + 1 == self.part).then_some(name))
            .collect();
        let mut prefix = base.to_vec();
        prefix.extend(["--no-fail-fast", "--", "--exact"].map(str::to_owned));
        let initial: usize = prefix.iter().map(|s| s.encode_utf16().count() + 3).sum();
        let mut units = initial;
        let mut command = prefix.clone();
        let mut commands = Vec::new();
        for name in &names {
            let next = name.encode_utf16().count() + 3;
            if initial + next > ARGUMENT_UNITS {
                return Err("native test name exceeds the command argument budget".into());
            }
            if units + next > ARGUMENT_UNITS {
                commands.push(command);
                command = prefix.clone();
                units = initial;
            }
            command.push(name.clone());
            units += next;
        }
        // An empty selection must execute nothing; an empty harness filter
        // would silently run the entire suite on every shard.
        if command.len() > prefix.len() {
            commands.push(command);
        }
        Ok((names, commands))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_exact_filters_execute_the_expected_real_harness_cases() {
        let cases = [
            "tests::native_target_contract_maps_names_and_binaries",
            "tests::release_identity_validation_is_strict",
        ];
        let listing = cases
            .iter()
            .map(|s| format!("{s}: test\n"))
            .collect::<String>();
        let (_, commands) = Shard::parse("1/1")
            .unwrap()
            .commands(&["test".into()], &listing)
            .unwrap();
        let command = &commands[0];
        let split = command.iter().position(|a| a == "--").unwrap();
        let result = Command::new(env::current_exe().unwrap())
            .args(&command[split + 1..])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(
            result.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(stdout.contains("2 passed; 0 failed"), "{stdout}");
    }
    #[test]
    fn shards_cover_every_listed_case_once_and_bound_windows_arguments() {
        let names: BTreeSet<_> = (0..900)
            .map(|i| {
                format!(
                    "storage::maintenance_{}::a_long_fixture_name_{i:04}",
                    "case".repeat(12)
                )
            })
            .collect();
        let listing = names
            .iter()
            .map(|n| format!("{n}: test\n"))
            .collect::<String>();
        let base = vec!["test".into(), "--workspace".into(), "--lib".into()];
        let mut seen = BTreeSet::new();
        for part in 1..=3 {
            let (selected, commands) = Shard { part, total: 3 }.commands(&base, &listing).unwrap();
            assert!(commands.len() > 1);
            let actual: Vec<_> = commands
                .iter()
                .flat_map(|c| {
                    c[c.iter().position(|s| s == "--exact").unwrap() + 1..]
                        .iter()
                        .cloned()
                })
                .collect();
            assert_eq!(actual, selected);
            for command in commands {
                assert!(
                    command
                        .iter()
                        .map(|s| s.encode_utf16().count() + 3)
                        .sum::<usize>()
                        <= ARGUMENT_UNITS
                );
            }
            for name in selected {
                assert!(seen.insert(name));
            }
        }
        assert_eq!(seen, names);
        assert!(Shard::parse("0/3").is_err());
        assert!(Shard::parse("4/3").is_err());
        assert!(Shard::parse("1/17").is_err());
        assert!(Shard::parse("1/3/4").is_err());
    }
    #[test]
    fn empty_shard_never_runs_an_unfiltered_suite() {
        let (selected, commands) = Shard::parse("1/3")
            .unwrap()
            .commands(&["test".into()], "0 tests, 0 benchmarks\n")
            .unwrap();
        assert!(selected.is_empty());
        assert!(commands.is_empty());
    }
}
