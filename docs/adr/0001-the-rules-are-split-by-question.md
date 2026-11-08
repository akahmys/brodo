# ADR-0001: The rules and the design are split by the question each document answers

- **Status**: Amended
- **Date**: 2026-10-16

Condensed on 2026-11-08 (this record's last decision): each decision is kept with its
reason and what it set aside; the text before is in git, this file as of `53f69d5`.

## Context

One document answering many questions is where facts get stated twice and drift apart,
and agents forget rules that sit far from the work they govern. A status file updated
every session, kept apart from the plan, drifted from it.

## Decision

The human decided, on 2026-10-16 unless dated otherwise.

- **One question, one document.** AGENTS.md lists the documents and what outranks what.
  The rules are split by the phase of work they govern (planning, coding, testing,
  auditing); the design is in ARCHITECTURE.md, the plan and the status in ROADMAP.md, and
  the reasons in these records. Documents, code and commit messages are in English. Rules
  state what enforces them.
- **The status** is ROADMAP.md §1, overwritten at the end of each unit of work, so that
  status and plan are read in one place. `commit_msg_check.py`, in the hook and the audit,
  refuses a commit that adds an ADR or ticks a box in ROADMAP.md without changing §1
  (2026-11-05: five commits had ended units and left it as it was).
- **A rule changes** only with the human's approval and a record here, in one commit with
  its enforcement; the `commit-msg` hook asks a commit touching a rule file to name an ADR
  it stages. The rule files are listed once, `project.RULE_FILES`, which the hook reads and
  `documents.py` holds every path AGENTS.md names to (2026-11-02).
- **References resolve** (2026-10-17). `documents.py` fails on a `NAME.md §N` naming a
  document without a heading so numbered, a section found by a number followed by a dot or
  a space, so that `4.2` is not `4.2.1`; it reads every committable file that is UTF-8 with
  no NUL, listed once (2026-11-02: lists by suffix had left `.cpp`, `.tsv` and the hooks
  unread).
- **A crate's README** (2026-10-20, 2026-10-22) answers what the crate does and how it is
  used, holds no rule or design, and is its documentation's front page, read in by
  `include_str!` in a `#![doc]` attribute of its `lib.rs`, so that its code runs as a
  doctest and cannot drift from the API. `documents.py` checks that every crate in
  `LAYERS` has one, read in so: brodo had none until asked for one.
- **What an ADR claims** (PLANNING.md §2). Its claims about the code as it is are
  established by running something before the human decides, and one found wrong while
  implementing goes back as an amendment before the work lands (2026-10-20: two ADRs'
  claims, the complement's Betti numbers and the header's room, were wrong). A claim
  holding for every input is measured on each kind of input the design accepts, badly
  oriented, holed or degenerate meshes among them (2026-10-22). A number it sets is tried
  on the case at its edge (2026-10-26). It states as the human's decision only a point the
  human answered in the session (2026-10-29). An open question is answered by a
  measurement, a spike's or another, or set aside for now by the human (2026-11-02).
- **What a spike measures** (PLANNING.md §3): a method's cost at the size and on the shapes
  it will meet, about 10⁵ triangles and slivers (2026-10-26: a report went from 1.69 s to
  92 s once built); before timing a case, that it holds what is said to be measured
  (2026-10-29: a box timed at a sphere's centre met no leaf); and for a format, the inputs
  a reader will refuse or skip as well as those it reads (2026-11-01). Each held by review.
- **Records** (docs/adr/README.md): the context and amendments in the past tense, the
  decision possibly in the present, with no limit of length; a record reversed or narrowed
  in part is amended to name the record that did it (2026-11-02).
- **An ADR records decisions, and may be condensed** (2026-11-08). At 1.0 the records held
  338 KB; ADR-0023 had 21 amendments and ADR-0019 15, eleven of them a milestone's run
  reported, whose facts stood also in the commit, ROADMAP.md and `.cargo/mutants.toml`,
  and a decision was found only by reading every amendment. An ADR records a decision that
  changes the design or a rule, with its reason and what it set aside (PLANNING.md §2);
  what applies a rule, a run's results, a test written for a mutant, a baseline kept or
  recorded again, is stated in the commit and, while open, in ROADMAP.md, and an exclusion
  approved for `.cargo/mutants.toml` adds one line to ADR-0023's list. A record may be
  condensed once, as the owner decides: its decisions kept with their dates, reasons and
  the options set aside, its reports left to git, the record naming the commit holding the
  text before, its number, title and file kept so that every reference holds. Fourteen were
  condensed on 2026-11-08, those of most amendments; ADR-0007, ADR-0010 and ADR-0044, on
  licences and sources, were left whole, their history being their evidence.

## Consequences

The design changes only by ADR. Most of these rules are held by review alone; what
`documents.py` and `commit_msg_check.py` check is AUDITING.md's.
