use super::helpers::watch_matches_any_pattern;
use crate::product_cli::{
    materialize::{
        default_excluded_parts, read_codebase_graph_ignore, read_materialization_config_rules,
        MaterializeOptions,
    },
    setup::GraphStatePaths,
};
use notify::{
    event::{AccessKind, AccessMode},
    Event, EventKind,
};
use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(in crate::product_cli) struct WatchEventFilter {
    pub(in crate::product_cli) source_root: PathBuf,
    pub(in crate::product_cli) current_dir: PathBuf,
    pub(in crate::product_cli) excluded_parts: BTreeSet<String>,
    pub(in crate::product_cli) include_patterns: Vec<String>,
    pub(in crate::product_cli) exclude_patterns: Vec<String>,
    pub(in crate::product_cli) ignore_patterns: Vec<String>,
}

impl WatchEventFilter {
    pub(in crate::product_cli) fn from_options(
        source_root: &Path,
        options: &MaterializeOptions,
    ) -> Result<Self, String> {
        let paths = GraphStatePaths::derive(source_root);
        let config_rules = read_materialization_config_rules(&paths.config_path)?;
        let mut include_patterns = config_rules.include_patterns;
        include_patterns.extend(options.include_patterns.clone());
        let mut exclude_patterns = config_rules.exclude_patterns;
        exclude_patterns.extend(options.exclude_patterns.clone());
        Ok(Self {
            source_root: source_root.to_path_buf(),
            current_dir: env::current_dir().unwrap_or_else(|_| source_root.to_path_buf()),
            excluded_parts: default_excluded_parts().into_iter().collect(),
            include_patterns,
            exclude_patterns,
            ignore_patterns: read_codebase_graph_ignore(source_root)?,
        })
    }

    pub(in crate::product_cli) fn relevant_paths(&self, event: &Event) -> BTreeSet<String> {
        if !watch_event_refreshes(event) {
            return BTreeSet::new();
        }
        event
            .paths
            .iter()
            .filter_map(|path| self.relevant_path(path))
            .collect()
    }

    pub(in crate::product_cli) fn relevant_path(&self, path: &Path) -> Option<String> {
        let relative = self.relative_event_path(path)?;
        if relative.as_os_str().is_empty() {
            return None;
        }
        if relative.components().any(|component| {
            self.excluded_parts
                .contains(component.as_os_str().to_string_lossy().as_ref())
        }) {
            return None;
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        if self.ignored_by_patterns(&relative) {
            None
        } else {
            Some(relative)
        }
    }

    pub(in crate::product_cli) fn relative_event_path(&self, path: &Path) -> Option<PathBuf> {
        if let Ok(relative) = path.strip_prefix(&self.source_root) {
            return Some(relative.to_path_buf());
        }
        if path.is_relative() {
            let absolute = self.current_dir.join(path);
            if let Ok(relative) = absolute.strip_prefix(&self.source_root) {
                return Some(relative.to_path_buf());
            }
            return Some(path.to_path_buf());
        }
        None
    }

    pub(in crate::product_cli) fn ignored_by_patterns(&self, relative_path: &str) -> bool {
        if !self.include_patterns.is_empty()
            && !watch_matches_any_pattern(relative_path, &self.include_patterns)
        {
            return true;
        }
        watch_matches_any_pattern(relative_path, &self.ignore_patterns)
            || watch_matches_any_pattern(relative_path, &self.exclude_patterns)
    }
}

pub(in crate::product_cli) fn watch_event_refreshes(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
            | EventKind::Other
            | EventKind::Access(AccessKind::Close(AccessMode::Write))
    )
}
