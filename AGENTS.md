# Governance

Where each thing is written down, what outranks what, and what may be read. Read this
first, every session.

## Hierarchy of truth

```
1. The human's instruction in the session
   └── 2. Measurement: running the code, and the output of whatever it is compared with
        └── 3. This file
             └── 4. The phase documents, in the order work happens:
                    PLANNING.md → CODING.md → TESTING.md → AUDITING.md
                 and ARCHITECTURE.md, the design they are applied to
```

When an instruction conflicts with a rule here, say so before acting on it.

**Measurement outranks documentation.** A claim about the code is established by running
something, not by reading a document, a name or a comment. A claim about anything outside
the repository (a library, a format, a paper) is established by its documentation, its
output or the paper itself, read within the limits of "Sources" below, never by recollection.
A document that disagrees with a measurement is corrected.

The phase documents state rules; `ARCHITECTURE.md` states the design. They do not overlap.
If a rule and the design seem to conflict, stop and ask the human.

## Which document answers what

| Document | Answers | Does **not** contain |
| :--- | :--- | :--- |
| [AGENTS.md](AGENTS.md) | Where is everything, what outranks what, and what may be read? | Phase rules |
| [PLANNING.md](PLANNING.md) | How is work sized, started, stopped, and decided? | Any specific plan |
| [CODING.md](CODING.md) | What must code satisfy, and where do files go? | Why the design is this shape |
| [TESTING.md](TESTING.md) | What must pass before a change lands, and how is a version numbered? | Test results |
| [AUDITING.md](AUDITING.md) | What is checked mechanically, and by what? | The rules being checked |
| [ARCHITECTURE.md](ARCHITECTURE.md) | What is the design **now**? | History, rules |
| [ROADMAP.md](ROADMAP.md) | Where is the work, what is next, what is open? | Rules, design |
| [PROVENANCE.md](PROVENANCE.md) | What did each source file draw on? | Rules, design |
| [CHANGELOG.md](CHANGELOG.md) | What changed in each version, and what breaks a user? | Plans |
| [README.md](README.md) | What is the project, and how is it built? | Rules, design |
| [docs/adr/](docs/adr/README.md) | Why is a rule or the design the way it is? | The present design |

Claude Code loads this file at the start of a session only while no `CLAUDE.md`,
`.claude/CLAUDE.md` or `CLAUDE.local.md` exists, so the repository has none; the audit
fails on one. A subagent starts without this file: a task handed to one carries the rules
it needs, "Sources" always among them.

## Sources

brodo is MIT OR Apache-2.0 (ADR-0010). Code under a copyleft licence must not reach it,
not even as a translation or a memory of how something was written (ADR-0007).

| Rule | Requirement | Enforced by |
| :--- | :--- | :--- |
| **S1** | Never read, translate or port the source of **the solver**, **OpenVDB** or **OpenFOAM**, nor the solver's or the design platform's design documents or code. Read only brodo's own files, papers, public documentation (NanoVDB's for its data-structure ideas only), and a dependency's public API and documentation | `scripts/hooks/provenance_guard.py` blocks paths and URLs naming them; a name written another way, **nothing** |
| **S2** | Code is copied from another crate only when its licence is on `deny.toml`'s allow-list, with its notice kept, and recorded in PROVENANCE.md | nothing |
| **S3** | Every source file under `crates/*/src/` has a row in PROVENANCE.md naming the papers and documents it draws on and for what, added in the commit that adds the file | `scripts/audit/documents.py` (the row exists); its truth, **nothing** |
| **S4** | brodo knows nothing of its users: no CFD, solver or design-platform concept in its code. A feature outside ARCHITECTURE.md §1's scope is recorded as a request (ROADMAP.md §4), not implemented | nothing |

## Writing rules

1. **One fact, one home.** Link to it instead of restating it.
2. **Present tense here and in `ARCHITECTURE.md`; past tense in `docs/adr/`.** A reversal
   is recorded in an ADR, not narrated in the design.
3. **A quoted figure carries its date or the command that derives it.**
4. **A rule names what enforces it, or says "nothing".** An unchecked rule is a comment.
5. **One ADR, one topic**, amended rather than multiplied
   ([docs/adr/README.md](docs/adr/README.md)).
6. **English** for code, comments, documents and commit messages.
7. **Concise.** Say the rule and its reason; leave out history and alternatives (they
   belong in an ADR).

`scripts/audit/documents.py` checks rules 1 (links resolve), 2 (dated past tense in the two
present-tense documents) and 6, and the ADR index. The rest are held by review.

## The gate

```bash
./scripts/audit/verify_compliance.sh   # must end "=== AUDIT PASSED ==="
```

It runs every check and then the tests. Read its last line, not the first.
[TESTING.md](TESTING.md) §4 has the whole gate.

## Changing the rules

A rule is anything that states or enforces one: this file, the phase documents, the lint
levels in `Cargo.toml`, `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`, `deny.toml`,
`.betterleaks.toml`, `.claude/settings.json`, and everything in `scripts/audit/` and
`scripts/hooks/`, `scripts/audit/project.py` and exemption lists included.

1. **The human decides.** An agent proposes a change, tightening or loosening, and makes
   it only once the human approves it in the session.
2. **It is recorded** in the ADR on its topic, amended or new.
3. **It lands whole**, in one commit: the rule's text, its enforcement, the AUDITING.md
   table, and the ADR. The check is shown to fire by breaking the rule once (TESTING.md T3).
4. **Numbers are never reused.** A retired rule leaves a gap, and its ADR says why.

The `commit-msg` hook holds part of this: a commit that touches any of these files must
name an ADR and stage one. Whether the human approved, **nothing** checks.

## Principles

1. **Correctness is shown by a test that can fail.** Code is right when a check that would
   catch the defect passes, not when it looks right.
2. **Reproducibility is a feature.** The same input and binary give the same output, the
   same bytes for a written grid, whatever the thread count.
3. **Do not guess and do not decide what is open.** An open question
   ([ROADMAP.md](ROADMAP.md) §3) is answered by a spike and an ADR, not by an
   implementation. When unsure, ask the human.
