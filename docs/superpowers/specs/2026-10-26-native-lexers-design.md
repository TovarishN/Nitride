# Generated lexical execution

The user approved proceeding toward Roslyn full-parse speed, starting with generated lexical execution. Generate ordinary Rust scanners from admitted lexical grammar IR, alongside the unchanged Expr grammar. This removes generic expression dispatch and nested token-node bookkeeping without hand-coding C# tokens. Preserve ordered choice, possessive repeats, predicates, exclusions, UTF-8 reads and dependency observations.

Builder decisions: admit only lexical rules with no owner, no Pratt expression, and a bounded acyclic graph of lexical references. Unsupported shapes keep the interpreter. Generate ASCII class bitsets and normalized Unicode interval lookup. Scanner state is local, with bounded work/depth/physical-stack checks; exhausted or unsupported speculative execution must not mutate the parser. Ordinary mismatch restores the token start but commits work and read observations; success commits its end as well. No CST nodes inside tokens.

Expose an opt-in compiled snapshot entry point through generated LEXERS bound to its static Language. Preserve existing Language, ParseOptions and default APIs. Cold contiguous text may use scanners; edited ropes and token_children mode use the interpreter. A compiled parse error reruns the ordinary entry point to preserve established error and recovery-budget behavior. Recovery probes remain unchanged. Snapshots do not retain scanner scratch state or global caches.

Success means full syntax/diagnostic equivalence on frozen corpora and generated lexical edge cases, bounded retained memory, and measured cold speed benefit against the frozen Stage17 executable on matching hashes. Roslyn parity must be assessed honestly, never inferred from instruction counts. No integration or default-backend cutover.

Review correction: over-depth recognition facts are declined and recognized afresh during edits, since generated and interpreted choices can have different depth. Actual execution still checks the requested limits. SnapshotStats gains native call/work counters; Language and ParseOptions retain their shapes.
