use crate::protocol::LanguageProfile;
use std::collections::BTreeMap;

pub(crate) struct ProfileSet<'a> {
    by_language: BTreeMap<&'a str, &'a LanguageProfile>,
    suffix_to_language: BTreeMap<String, &'a str>,
}

impl<'a> ProfileSet<'a> {
    pub(crate) fn new(profiles: &'a [LanguageProfile]) -> Self {
        let mut by_language = BTreeMap::new();
        let mut suffix_to_language = BTreeMap::new();
        for profile in profiles {
            by_language.insert(profile.language.as_str(), profile);
            for suffix in &profile.suffixes {
                suffix_to_language.insert(suffix.to_lowercase(), profile.language.as_str());
            }
        }
        Self {
            by_language,
            suffix_to_language,
        }
    }

    pub(crate) fn language_for_path(&self, path: &std::path::Path) -> Option<String> {
        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(|extension| {
                let suffix = format!(".{}", extension.to_lowercase());
                self.suffix_to_language.get(&suffix).copied()
            })
            .map(str::to_string)
    }

    pub(crate) fn profile_for_language(&self, language: &str) -> Option<&'a LanguageProfile> {
        self.by_language.get(language).copied()
    }
}
