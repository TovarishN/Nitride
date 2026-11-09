//! A recursive budget oracle independent of the production pending-frame walk.
use super::*;
use crate::syntax::arena::{ArenaBuilder, Child};

type Emitted = (u32, u16, [u32; 2], u32, Vec<u32>);
struct Oracle {
    limit: u64,
    work: u64,
    edges: u32,
    emitted: Vec<Emitted>,
}
impl Oracle {
    fn tick(&mut self, span: [u32; 2]) -> Result<(), Diagnostic> {
        self.work += 1;
        if self.work > self.limit {
            return Err(Diagnostic::new(
                "limit",
                span,
                "occurrence projection work limit exceeded",
            ));
        }
        Ok(())
    }
    fn walk(&mut self, view: BorrowedNodeView<'_, '_>) -> Result<u32, Diagnostic> {
        let span = view.span();
        self.tick(span)?;
        let children: Vec<_> = view.children().map(|(_, child)| child).collect();
        if self.work + children.len() as u64 > self.limit {
            return Err(Diagnostic::new(
                "limit",
                span,
                "occurrence projection child scheduling work limit exceeded",
            ));
        }
        self.work += children.len() as u64;
        let ids: Vec<_> = children
            .into_iter()
            .map(|child| self.walk(child))
            .collect::<Result<_, _>>()?;
        self.tick(span)?;
        let id = self.emitted.len() as u32;
        self.emitted
            .push((id, view.rule(), span, self.edges, ids.clone()));
        self.edges += ids.len() as u32;
        Ok(id)
    }
}

#[test]
fn projection_prefixes_and_errors_match_recursive_budget_oracle() {
    // The same branch is placed twice, separated by a leaf and source gaps.
    let mut arena = ArenaBuilder::new(vec![""].into());
    let leaf = arena.add(0, 1, vec![]).unwrap();
    let pair = arena
        .add(
            0,
            3,
            vec![
                Child {
                    node: leaf,
                    extent: 1,
                    label: 0,
                },
                Child {
                    node: leaf,
                    extent: 2,
                    label: 0,
                },
            ],
        )
        .unwrap();
    let root = arena
        .add(
            0,
            9,
            vec![
                Child {
                    node: pair,
                    extent: 3,
                    label: 0,
                },
                Child {
                    node: leaf,
                    extent: 2,
                    label: 0,
                },
                Child {
                    node: pair,
                    extent: 4,
                    label: 0,
                },
            ],
        )
        .unwrap();
    let snapshot = ParseSnapshot::from_parts(
        super::tests::language(),
        SourceText::new("x x x x x").unwrap(),
        arena.finish(),
        root,
        0,
        SnapshotMode::Strict(crate::ParseOptions::default()),
    );
    for limit in 0..32 {
        let mut oracle = Oracle {
            limit,
            work: 0,
            edges: 0,
            emitted: vec![],
        };
        let expected = oracle
            .walk(snapshot.root())
            .map_err(|e| (e.span, e.message));
        let mut emitted = Vec::new();
        let actual = snapshot
            .visit_postorder(limit, |id, node, children| {
                assert_eq!(node.child_count as usize, children.len());
                emitted.push((
                    id,
                    node.rule,
                    node.span,
                    node.first_child,
                    children.to_vec(),
                ));
            })
            .map_err(|e| (e.span, e.message));
        assert_eq!(actual, expected, "budget {limit}");
        assert_eq!(emitted, oracle.emitted, "budget {limit}");
    }
}
