//! Disk-backed diagnostics for workspace languages with grammar rule checks.
use super::*;

fn has_rule_checks(language: &crate::registry::LanguageState) -> bool {
    language
        .current
        .as_ref()
        .is_some_and(|loaded| !loaded.language.language().rule_checks.is_empty())
}

impl<W: Write> Server<W> {
    fn is_rule_source(&self, path: &Path) -> bool {
        self.registry.for_path(path).is_some_and(has_rule_checks)
            && !crate::registry::language_input_path(&self.registry, path)
    }

    fn disk_is_open(&self, uri: &str) -> bool {
        self.documents.contains_key(uri)
            || self.documents.keys().any(|open_uri| {
                uri::to_path(open_uri).is_some_and(|path| grammar_index::default_uri(&path) == uri)
            })
    }

    pub(super) fn disk_source(&self, uri: &str) -> Option<PathBuf> {
        let path = uri::to_path(uri)?;
        (self.is_rule_source(&path) && crate::registry::workspace_file(&self.folders, &path))
            .then_some(path)
    }

    pub(super) fn queue_disk_source(&mut self, path: &Path) -> Option<String> {
        let uri = grammar_index::default_uri(path);
        if !self.disk_published.contains(&uri) && self.disk_source(&uri).is_none() {
            return None;
        }
        if self.disk_is_open(&uri) {
            return None;
        }
        self.cancel_disk_job(&uri);
        self.disk_pending.insert(uri.clone());
        Some(uri)
    }

    pub(super) fn close_document_diagnostics(&mut self, uri: &str) -> io::Result<()> {
        let had_analysis = self.analyses.remove(uri).is_some();
        let disk = uri::to_path(uri).and_then(|path| self.queue_disk_source(&path));
        match disk {
            Some(disk_uri) if had_analysis => {
                self.disk_published.insert(disk_uri);
                Ok(())
            }
            None if had_analysis => self.publish(uri, vec![]),
            _ => Ok(()),
        }
    }

    pub(super) fn refresh_disk_sources(&mut self) {
        if self.shutdown {
            return;
        }
        let open: BTreeSet<_> = self
            .documents
            .keys()
            .filter_map(|uri| uri::to_path(uri))
            .map(|path| grammar_index::default_uri(&path))
            .collect();
        let mut pending: BTreeSet<_> = self.disk_published.difference(&open).cloned().collect();
        if self.registry.languages().iter().any(has_rule_checks) {
            crate::registry::visit_workspace_files(&self.folders, |entry| {
                let path = entry.path();
                if self.is_rule_source(&path) {
                    let uri = grammar_index::default_uri(&path);
                    if !open.contains(&uri) {
                        pending.insert(uri);
                    }
                }
            });
        }
        self.disk_pending = pending;
    }

    pub(super) fn commit_disk_diagnostics(
        &mut self,
        uri: &str,
        analysis: Option<&Analysis>,
    ) -> io::Result<()> {
        let diagnostics: Vec<_> = analysis
            .into_iter()
            .flat_map(|analysis| {
                analysis
                    .problems
                    .iter()
                    .map(|p| problem_diagnostic(&analysis.lines, p))
            })
            .collect();
        if diagnostics.is_empty() {
            if !self.disk_published.remove(uri) {
                return Ok(());
            }
        } else {
            self.disk_published.insert(uri.to_owned());
        }
        self.publish(uri, diagnostics)
    }
}
