# Direct snapshot editor projection

Avoid the temporary full SyntaxTree for syntax-only workspace analysis that has
no declaration/reference metadata and no rule checks. Retain the final editor
Tree's exact postorder nodes, child IDs, root, spans and grammar owner. This is
a bounded projection optimization, not elimination of the editor's dense tree.
Typed documents and syntax-only metadata/check cases retain the established path.

Add a bounded core postorder visitor with scoped child IDs. It reads immutable
snapshot occurrences without exporting source text, edge labels or recovery
coverage. Preserve work/capacity checks and placement offsets. Add a runtime
snapshot callback that clones the shared immutable handle under the store lock
and releases that lock before calling consumers. It must support reentrant reads
or edits while retaining language/snapshot lifetimes, without unsafe code.

Use that visitor to build the final editor nodes/edges directly. Syntax-only
problems and repair fixes come from snapshot sidecars. On projection failure,
retain the established materialized-analysis fallback. Frontends are unchanged.

Compare direct output with the legacy materialized path for valid/recovery,
Unicode, placements/edits, checked and declaration-bearing languages, and both
palette/standard tokens, folding, selection, formatting, completion and hints.
Retain a performance change only after matched native release measurements show
an improvement in total cached analysis, without claiming typed or GUI gains.
