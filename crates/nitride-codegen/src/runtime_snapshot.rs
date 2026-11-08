//! Owning handles for snapshots of languages prepared at runtime.
//!
//! The cell keeps language tables alive; its dependent store borrows them.
//! Handles never expose the store's lifetime. Removing the final handle frees
//! its slot, so edit history is retained only by readers that requested it.
use crate::LoadedLanguage;
use nitride_core::*;
use std::sync::{Arc, Mutex, Weak};

#[derive(Clone, Copy, Default)]
pub struct RuntimeAnalysisTimings {
    pub materialize_ns: u128,
    pub semantic_ns: u128,
}

struct Store<'g> {
    slots: Vec<Option<BorrowedParseSnapshot<'g>>>,
    free: Vec<usize>,
}
impl<'g> Store<'g> {
    fn insert(&mut self, snapshot: BorrowedParseSnapshot<'g>) -> usize {
        if let Some(id) = self.free.pop() {
            self.slots[id] = Some(snapshot);
            id
        } else {
            self.slots.push(Some(snapshot));
            self.slots.len() - 1
        }
    }
}
type Snapshots<'g> = Mutex<Store<'g>>;
self_cell::self_cell! {
    struct Session {
        owner: Arc<LoadedLanguage>,
        #[not_covariant]
        dependent: Snapshots,
    }
}
struct Slot {
    session: Weak<Session>,
    id: usize,
}
impl Drop for Slot {
    fn drop(&mut self) {
        if let Some(session) = self.session.upgrade() {
            session.with_dependent(|_, store| {
                let mut store = store.lock().unwrap_or_else(|e| e.into_inner());
                if store.slots[self.id].take().is_some() {
                    store.free.push(self.id);
                }
            });
        }
    }
}

/// A reader of immutable syntax retaining its runtime language owner.
/// Clones share the same reader; edits create new readers in the same session.
#[derive(Clone)]
pub struct RuntimeSnapshot {
    // Drop the slot before the strong session reference.
    slot: Arc<Slot>,
    session: Arc<Session>,
}
impl RuntimeSnapshot {
    pub fn parse(
        owner: Arc<LoadedLanguage>,
        source: &str,
        limits: Limits,
        mode: SnapshotMode,
    ) -> Result<Self, Diagnostic> {
        let session = Arc::new(Session::try_new(owner, |owner| {
            let snapshot = parse_borrowed_snapshot(owner.language(), source, limits, mode)?;
            Ok::<_, Diagnostic>(Mutex::new(Store {
                slots: vec![Some(snapshot)],
                free: vec![],
            }))
        })?);
        Ok(Self::handle(session, 0))
    }

    fn handle(session: Arc<Session>, id: usize) -> Self {
        Self {
            slot: Arc::new(Slot {
                session: Arc::downgrade(&session),
                id,
            }),
            session,
        }
    }

    pub fn same_language(&self, owner: &Arc<LoadedLanguage>) -> bool {
        Arc::ptr_eq(self.session.borrow_owner(), owner)
    }

    pub fn edit(&self, edit: &TextEdit, limits: Limits) -> Result<Self, Diagnostic> {
        let id = self.session.with_dependent(|_, store| {
            let mut store = store.lock().unwrap_or_else(|e| e.into_inner());
            let snapshot = store.slots[self.slot.id]
                .as_ref()
                .expect("live snapshot slot");
            let next = snapshot.edit(edit, limits)?;
            Ok::<_, Diagnostic>(store.insert(next))
        })?;
        Ok(Self::handle(self.session.clone(), id))
    }

    /// Materialize a flat tree for a scoped consumer. No lock is held while
    /// the callback runs; it may inspect or edit another handle in this session.
    pub fn with_tree<R>(
        &self,
        f: impl for<'g> FnOnce(&SyntaxTree<'g>) -> R,
    ) -> Result<R, Diagnostic> {
        self.session.with_dependent(|_, store| {
            let tree = {
                let store = store.lock().unwrap_or_else(|e| e.into_inner());
                store.slots[self.slot.id]
                    .as_ref()
                    .expect("live snapshot slot")
                    .materialize()?
            };
            Ok(f(&tree))
        })
    }

    /// Materialize once and run the established whole-document checker,
    /// including structural checks on syntax-only languages.
    pub fn with_analyzed<R>(
        &self,
        limits: Limits,
        f: impl for<'g> FnOnce(&SyntaxTree<'g>, RecoveredAnalysis<'g>) -> R,
    ) -> Result<R, Diagnostic> {
        self.with_analyzed_timed(limits, |tree, checked, _| f(tree, checked))
    }

    /// Phase measurements for benchmark and editor diagnostics tooling.
    pub fn with_analyzed_timed<R>(
        &self,
        limits: Limits,
        f: impl for<'g> FnOnce(&SyntaxTree<'g>, RecoveredAnalysis<'g>, RuntimeAnalysisTimings) -> R,
    ) -> Result<R, Diagnostic> {
        self.session.with_dependent(|owner, store| {
            let started = std::time::Instant::now();
            let parsed = {
                let store = store.lock().unwrap_or_else(|e| e.into_inner());
                let snapshot = store.slots[self.slot.id]
                    .as_ref()
                    .expect("live snapshot slot");
                RecoveredParse {
                    tree: snapshot.materialize()?,
                    diagnostics: snapshot.diagnostics().to_vec(),
                    stats: RecoveryStats::default(),
                }
            };
            let materialize_ns = started.elapsed().as_nanos();
            let started = std::time::Instant::now();
            let checked = compile_recovering(owner.language(), &parsed, limits);
            let semantic_ns = started.elapsed().as_nanos();
            Ok(f(
                &parsed.tree,
                checked,
                RuntimeAnalysisTimings {
                    materialize_ns,
                    semantic_ns,
                },
            ))
        })
    }

    /// The program borrows the retained language and cannot escape this callback.
    pub fn with_compiled<R>(
        &self,
        limits: Limits,
        f: impl for<'g> FnOnce(RecoveredAnalysis<'g>) -> R,
    ) -> Result<R, Diagnostic> {
        self.with_analyzed(limits, |_, checked| f(checked))
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.session.with_dependent(|_, store| {
            store.lock().unwrap_or_else(|e| e.into_inner()).slots[self.slot.id]
                .as_ref()
                .expect("live snapshot slot")
                .diagnostics()
                .to_vec()
        })
    }

    pub fn stats(&self) -> SnapshotStats {
        self.session.with_dependent(|_, store| {
            store.lock().unwrap_or_else(|e| e.into_inner()).slots[self.slot.id]
                .as_ref()
                .expect("live snapshot slot")
                .stats()
                .clone()
        })
    }

    pub fn storage_stats(&self) -> StorageStats {
        self.session.with_dependent(|_, store| {
            store.lock().unwrap_or_else(|e| e.into_inner()).slots[self.slot.id]
                .as_ref()
                .expect("live snapshot slot")
                .storage_stats()
        })
    }
}
