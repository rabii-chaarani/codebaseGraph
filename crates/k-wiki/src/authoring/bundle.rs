use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::RwLock,
};

use yaml_serde::Value as YamlValue;

use super::{
    page::{
        CreateBundleRequest, CreateBundleResult, CreatePageRequest, CreatePageResult,
        PageFrontmatter, PopulatePageRequest, PopulatePageResult,
    },
    write::{
        content_hash, parse_document, render_document, write_atomically, AuthoringError,
        AuthoringValidator, RefreshEvent, RefreshNotifier, RefreshOperation, ValidationTarget,
        ValidationTargetKind,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryRoot {
    pub id: String,
    pub root_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BundleRoot {
    pub id: String,
    pub repository_id: String,
    pub root_path: PathBuf,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthoringConfig {
    pub repositories: Vec<RepositoryRoot>,
    pub bundles: Vec<BundleRoot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedRepositoryRoot {
    id: String,
    root_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedBundleRoot {
    id: String,
    repository_id: String,
    root_path: PathBuf,
    bundle_path: String,
}

#[derive(Debug)]
pub struct AuthoringService<V, R> {
    repositories: BTreeMap<String, ResolvedRepositoryRoot>,
    bundles: RwLock<BTreeMap<String, ResolvedBundleRoot>>,
    validator: V,
    refresh_notifier: R,
}

impl<V, R> AuthoringService<V, R>
where
    V: AuthoringValidator,
    R: RefreshNotifier,
{
    pub fn new(
        config: AuthoringConfig,
        validator: V,
        refresh_notifier: R,
    ) -> Result<Self, AuthoringError> {
        let repositories = resolve_repositories(config.repositories)?;
        let bundles = resolve_bundles(&repositories, config.bundles)?;
        Ok(Self {
            repositories,
            bundles: RwLock::new(bundles),
            validator,
            refresh_notifier,
        })
    }

    pub fn create_bundle(
        &self,
        request: CreateBundleRequest,
    ) -> Result<CreateBundleResult, AuthoringError> {
        let repository = self.repository(&request.repository_id)?;
        if request.bundle_id.trim().is_empty() {
            return Err(AuthoringError::invalid_request(
                "bundle_id must not be blank",
            ));
        }
        if request.okf_version.trim().is_empty() {
            return Err(AuthoringError::invalid_request(
                "okf_version must not be blank",
            ));
        }

        {
            let bundles = self
                .bundles
                .read()
                .map_err(|_| AuthoringError::invalid_request("bundle registry is poisoned"))?;
            if bundles.contains_key(&request.bundle_id) {
                return Err(AuthoringError::bundle_exists(&request.bundle_id));
            }
        }

        let bundle_path = normalize_directory_path(&request.bundle_path)?;
        let target_root = resolve_path_within_root(&repository.root_path, &bundle_path, false)?;
        if target_root.exists() {
            return Err(AuthoringError::bundle_exists(&bundle_path));
        }

        let index_frontmatter = bundle_frontmatter(&request);
        let index_content = render_document(
            &index_frontmatter,
            request.body_markdown.as_deref().unwrap_or_default(),
        )?;
        let index_path = target_root.join("index.md");

        self.validator.validate(ValidationTarget {
            kind: ValidationTargetKind::BundleIndex,
            bundle_id: &request.bundle_id,
            source_path: "index.md",
            content: &index_content,
        })?;

        fs::create_dir_all(&target_root)
            .map_err(|error| AuthoringError::io("create bundle directory", &bundle_path, error))?;

        let hash = write_atomically(&index_path, &index_content)?;
        let configured_bundle = ResolvedBundleRoot {
            id: request.bundle_id.clone(),
            repository_id: request.repository_id.clone(),
            root_path: canonicalize_existing_directory(&target_root, &bundle_path)?,
            bundle_path: bundle_path.clone(),
        };
        self.bundles
            .write()
            .map_err(|_| AuthoringError::invalid_request("bundle registry is poisoned"))?
            .insert(configured_bundle.id.clone(), configured_bundle);

        self.refresh_notifier.notify(&RefreshEvent {
            operation: RefreshOperation::BundleCreated,
            bundle_id: request.bundle_id.clone(),
            source_path: "index.md".into(),
        });

        Ok(CreateBundleResult {
            bundle_id: request.bundle_id,
            repository_id: request.repository_id,
            bundle_path,
            index_path: "index.md".into(),
            content_hash: hash,
        })
    }

    pub fn create_page(
        &self,
        request: CreatePageRequest,
    ) -> Result<CreatePageResult, AuthoringError> {
        let bundle = self.bundle(&request.bundle_id)?;
        let resolved = resolve_page_target(&bundle.root_path, &request.page_path)?;
        if resolved.absolute_path.exists() {
            return Err(AuthoringError::concept_exists(&resolved.display_path));
        }

        let body = request
            .body_markdown
            .unwrap_or_else(|| default_page_body(request.title.as_deref(), &resolved.display_path));
        let frontmatter = PageFrontmatter {
            concept_type: request.concept_type,
            title: request.title,
            description: request.description,
            resource: request.resource,
            tags: request.tags,
            timestamp: request.timestamp,
            extensions: BTreeMap::new(),
        };
        let content =
            render_document(&serialize_frontmatter(frontmatter, BTreeMap::new())?, &body)?;

        self.validator.validate(ValidationTarget {
            kind: ValidationTargetKind::ConceptPage,
            bundle_id: &bundle.id,
            source_path: &resolved.display_path,
            content: &content,
        })?;

        let hash = write_atomically(&resolved.absolute_path, &content)?;
        self.refresh_notifier.notify(&RefreshEvent {
            operation: RefreshOperation::PageCreated,
            bundle_id: bundle.id.clone(),
            source_path: resolved.display_path.clone(),
        });

        Ok(CreatePageResult {
            bundle_id: bundle.id,
            source_path: resolved.display_path,
            content_hash: hash,
        })
    }

    pub fn populate_page(
        &self,
        request: PopulatePageRequest,
    ) -> Result<PopulatePageResult, AuthoringError> {
        let bundle = self.bundle(&request.bundle_id)?;
        let resolved = resolve_page_target(&bundle.root_path, &request.page_path)?;
        if !resolved.absolute_path.exists() {
            return Err(AuthoringError::concept_not_found(&resolved.display_path));
        }

        let metadata = fs::symlink_metadata(&resolved.absolute_path).map_err(|error| {
            AuthoringError::io("inspect existing concept", &resolved.display_path, error)
        })?;
        if metadata.file_type().is_symlink() {
            return Err(AuthoringError::path_outside_repository(
                &resolved.display_path,
            ));
        }

        let existing = fs::read_to_string(&resolved.absolute_path)
            .map_err(|error| AuthoringError::io("read concept", &resolved.display_path, error))?;
        let existing_hash = content_hash(existing.as_bytes());
        if let Some(expected) = request.expected_content_hash.as_deref() {
            if expected != existing_hash {
                return Err(AuthoringError::write_conflict(&resolved.display_path));
            }
        }

        let parsed = parse_document(&existing)?;
        let content = render_document(
            &serialize_frontmatter(request.frontmatter, parsed.frontmatter)?,
            &request.body_markdown,
        )?;

        self.validator.validate(ValidationTarget {
            kind: ValidationTargetKind::ConceptPage,
            bundle_id: &bundle.id,
            source_path: &resolved.display_path,
            content: &content,
        })?;

        let hash = write_atomically(&resolved.absolute_path, &content)?;
        self.refresh_notifier.notify(&RefreshEvent {
            operation: RefreshOperation::PagePopulated,
            bundle_id: bundle.id.clone(),
            source_path: resolved.display_path.clone(),
        });

        Ok(PopulatePageResult {
            bundle_id: bundle.id,
            source_path: resolved.display_path,
            content_hash: hash,
        })
    }

    fn repository(&self, repository_id: &str) -> Result<ResolvedRepositoryRoot, AuthoringError> {
        self.repositories
            .get(repository_id)
            .cloned()
            .ok_or_else(|| {
                AuthoringError::invalid_request(format!(
                    "repository `{repository_id}` is not configured"
                ))
            })
    }

    fn bundle(&self, bundle_id: &str) -> Result<ResolvedBundleRoot, AuthoringError> {
        self.bundles
            .read()
            .map_err(|_| AuthoringError::invalid_request("bundle registry is poisoned"))?
            .get(bundle_id)
            .cloned()
            .ok_or_else(|| AuthoringError::bundle_not_found(bundle_id))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedPageTarget {
    absolute_path: PathBuf,
    display_path: String,
}

fn resolve_repositories(
    repositories: Vec<RepositoryRoot>,
) -> Result<BTreeMap<String, ResolvedRepositoryRoot>, AuthoringError> {
    let mut resolved = BTreeMap::new();
    for repository in repositories {
        if repository.id.trim().is_empty() {
            return Err(AuthoringError::invalid_request(
                "repository id must not be blank",
            ));
        }
        let root_path = canonicalize_existing_directory(
            &repository.root_path,
            repository.root_path.to_string_lossy().as_ref(),
        )?;
        resolved.insert(
            repository.id.clone(),
            ResolvedRepositoryRoot {
                id: repository.id,
                root_path,
            },
        );
    }
    Ok(resolved)
}

fn resolve_bundles(
    repositories: &BTreeMap<String, ResolvedRepositoryRoot>,
    bundles: Vec<BundleRoot>,
) -> Result<BTreeMap<String, ResolvedBundleRoot>, AuthoringError> {
    let mut resolved = BTreeMap::new();
    for bundle in bundles {
        if bundle.id.trim().is_empty() {
            return Err(AuthoringError::invalid_request(
                "bundle id must not be blank",
            ));
        }
        let repository = repositories.get(&bundle.repository_id).ok_or_else(|| {
            AuthoringError::invalid_request(format!(
                "repository `{}` is not configured",
                bundle.repository_id
            ))
        })?;
        let root_path = if bundle.root_path.is_absolute() {
            canonicalize_existing_directory(
                &bundle.root_path,
                bundle.root_path.to_string_lossy().as_ref(),
            )?
        } else {
            let display = normalize_directory_path(&bundle.root_path.to_string_lossy())?;
            let resolved_path = resolve_path_within_root(&repository.root_path, &display, false)?;
            canonicalize_existing_directory(&resolved_path, &display)?
        };

        if !root_path.starts_with(&repository.root_path) {
            return Err(AuthoringError::path_outside_repository(&bundle.id));
        }

        let bundle_path = path_relative_to(&repository.root_path, &root_path)?;
        resolved.insert(
            bundle.id.clone(),
            ResolvedBundleRoot {
                id: bundle.id,
                repository_id: bundle.repository_id,
                root_path,
                bundle_path,
            },
        );
    }
    Ok(resolved)
}

fn canonicalize_existing_directory(path: &Path, display: &str) -> Result<PathBuf, AuthoringError> {
    let canonical = path
        .canonicalize()
        .map_err(|error| AuthoringError::io("resolve directory", display, error))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| AuthoringError::io("inspect directory", display, error))?;
    if !metadata.is_dir() {
        return Err(AuthoringError::invalid_request(format!(
            "`{display}` is not a directory"
        )));
    }
    Ok(canonical)
}

fn normalize_directory_path(input: &str) -> Result<String, AuthoringError> {
    let segments = normalize_relative_path(input, false)?;
    Ok(segments.join("/"))
}

fn resolve_page_target(
    bundle_root: &Path,
    page_path: &str,
) -> Result<ResolvedPageTarget, AuthoringError> {
    let mut segments = normalize_relative_path(page_path, true)?;
    let last = segments
        .pop()
        .ok_or_else(|| AuthoringError::invalid_request("page path must not be blank"))?;
    let file_name = normalize_markdown_file_name(&last)?;
    segments.push(file_name);
    let display_path = segments.join("/");
    reject_reserved_target(&display_path)?;
    let absolute_path = resolve_path_within_root(bundle_root, &display_path, true)?;
    Ok(ResolvedPageTarget {
        absolute_path,
        display_path,
    })
}

fn normalize_relative_path(
    input: &str,
    allow_file_name: bool,
) -> Result<Vec<String>, AuthoringError> {
    if input.trim().is_empty() {
        return Err(AuthoringError::invalid_request("path must not be blank"));
    }

    let path = Path::new(input);
    if path.is_absolute() {
        return Err(AuthoringError::path_outside_repository(input));
    }

    let mut segments = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => segments.push(value.to_string_lossy().to_string()),
            Component::CurDir if allow_file_name => {}
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(AuthoringError::path_outside_repository(input));
            }
        }
    }

    if segments.is_empty() {
        return Err(AuthoringError::invalid_request("path must not be blank"));
    }
    Ok(segments)
}

fn normalize_markdown_file_name(file_name: &str) -> Result<String, AuthoringError> {
    if file_name.is_empty() {
        return Err(AuthoringError::invalid_request(
            "page path must not be blank",
        ));
    }

    let path = Path::new(file_name);
    match path.extension().and_then(|value| value.to_str()) {
        None => Ok(format!("{file_name}.md")),
        Some(ext) if ext.eq_ignore_ascii_case("md") => Ok(file_name.to_string()),
        Some(_) => Err(AuthoringError::invalid_request(
            "page paths must end with `.md` or omit the extension",
        )),
    }
}

fn reject_reserved_target(path: &str) -> Result<(), AuthoringError> {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(file_name.as_str(), "index.md" | "log.md") {
        return Err(AuthoringError::invalid_request(format!(
            "`{path}` is a reserved OKF source path"
        )));
    }
    Ok(())
}

fn resolve_path_within_root(
    root: &Path,
    relative_path: &str,
    reject_final_symlink: bool,
) -> Result<PathBuf, AuthoringError> {
    let mut current = root.to_path_buf();
    let path = Path::new(relative_path);
    for (index, component) in path.components().enumerate() {
        let Component::Normal(name) = component else {
            return Err(AuthoringError::path_outside_repository(relative_path));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if reject_final_symlink && index == path.components().count() - 1 {
                    return Err(AuthoringError::path_outside_repository(relative_path));
                }
                let canonical = current
                    .canonicalize()
                    .map_err(|error| AuthoringError::io("resolve symlink", relative_path, error))?;
                if !canonical.starts_with(root) {
                    return Err(AuthoringError::path_outside_repository(relative_path));
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AuthoringError::io("inspect path", relative_path, error));
            }
        }
    }
    Ok(current)
}

fn path_relative_to(root: &Path, child: &Path) -> Result<String, AuthoringError> {
    let relative = child.strip_prefix(root).map_err(|_| {
        AuthoringError::invalid_request("configured bundle root is not inside its repository root")
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn bundle_frontmatter(request: &CreateBundleRequest) -> BTreeMap<String, YamlValue> {
    let mut frontmatter = BTreeMap::new();
    frontmatter.insert(
        "okf_version".into(),
        YamlValue::String(request.okf_version.clone()),
    );
    if let Some(title) = request
        .title
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        frontmatter.insert("title".into(), YamlValue::String(title.clone()));
    }
    frontmatter
}

fn serialize_frontmatter(
    frontmatter: PageFrontmatter,
    existing: BTreeMap<String, YamlValue>,
) -> Result<BTreeMap<String, YamlValue>, AuthoringError> {
    if frontmatter.concept_type.trim().is_empty() {
        return Err(AuthoringError::invalid_frontmatter(
            "frontmatter `type` must not be blank",
        ));
    }

    let mut merged = BTreeMap::new();
    let known_keys: BTreeSet<&str> = BTreeSet::from([
        "type",
        "title",
        "description",
        "resource",
        "tags",
        "timestamp",
    ]);

    for (key, value) in existing {
        if !known_keys.contains(key.as_str()) {
            merged.insert(key, value);
        }
    }

    for (key, value) in frontmatter.extensions {
        merged.insert(key, value);
    }

    merged.insert("type".into(), YamlValue::String(frontmatter.concept_type));
    if let Some(title) = frontmatter.title.filter(|value| !value.trim().is_empty()) {
        merged.insert("title".into(), YamlValue::String(title));
    }
    if let Some(description) = frontmatter
        .description
        .filter(|value| !value.trim().is_empty())
    {
        merged.insert("description".into(), YamlValue::String(description));
    }
    if let Some(resource) = frontmatter
        .resource
        .filter(|value| !value.trim().is_empty())
    {
        merged.insert("resource".into(), YamlValue::String(resource));
    }
    if !frontmatter.tags.is_empty() {
        merged.insert(
            "tags".into(),
            YamlValue::Sequence(
                frontmatter
                    .tags
                    .into_iter()
                    .map(YamlValue::String)
                    .collect(),
            ),
        );
    }
    if let Some(timestamp) = frontmatter
        .timestamp
        .filter(|value| !value.trim().is_empty())
    {
        merged.insert("timestamp".into(), YamlValue::String(timestamp));
    }
    Ok(merged)
}

fn default_page_body(title: Option<&str>, path: &str) -> String {
    let heading = title
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_title_from_path(path));
    format!("# {heading}\n")
}

fn default_title_from_path(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(path);
    stem.split(['-', '_'])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut normalized = first.to_uppercase().collect::<String>();
            normalized.push_str(chars.as_str());
            normalized
        })
        .collect::<Vec<_>>()
        .join(" ")
}
