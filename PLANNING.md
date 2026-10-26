# Planning

> **Phase: planning.** How work is sized, started, stopped and decided. The rules for the
> code itself are in [CODING.md](CODING.md).

## 1. A unit of work

1. **Start** by reading [ROADMAP.md](ROADMAP.md) §1 (status). `AGENTS.md` is already loaded.
2. **Size** a unit as about one test made to pass.
3. **End** by overwriting ROADMAP.md §1: what was done, the next step, and every
   unresolved problem. It is a snapshot, not a log; history is in git.
4. **Stop when stuck.** After three attempts that fail to explain a failure or a mismatch,
   stop. Record in ROADMAP.md §1 what fails, where, by how much, what was tried, and the
   hypothesis. Do not loosen the tolerance or the test.

## 2. What needs an ADR

An ADR ([docs/adr/README.md](docs/adr/README.md)) is written, and accepted by the human,
before any of these:

1. **A change to the design** ([ARCHITECTURE.md](ARCHITECTURE.md)).
2. **Answering an open question** (ROADMAP.md §3). Only a spike's measurement answers one;
   an implementation that settles it in passing is a defect.
3. **An exception** that a rule in CODING.md or TESTING.md says needs one.
4. **A second mechanism for a purpose one already serves.** The ADR removes one of them.
5. **A change to a rule** (AGENTS.md, "Changing the rules").

An ADR's claims about the code as it is (a size, a layout, what a function returns or what
can be counted) are established by running something before the human decides, as
AGENTS.md's "Measurement outranks documentation" asks; a claim found wrong while
implementing goes back to the human as an amendment before the work lands. A claim that
holds for every input is measured on each kind of input the design accepts, such as a
badly oriented, holed or degenerate mesh (ADR-0026), not only on well-made ones. A number
an ADR sets, a distance, a threshold or a bound, is tried on the case at its edge before
it is proposed. Held by review alone (ADR-0001).

## 3. Spikes

A spike is a throwaway prototype that answers an open question by measurement. It is run
only when the human asks, lives in `spikes/`, and is never imported by `crates/`. Its
deliverables are a measurement report and the ADR built on it. A report that weighs a
method's cost measures it at the size and on the shapes the method will meet: for a mesh,
one of about 10⁵ triangles and one of slivers. Held by review alone (ADR-0001).

## 4. Keeping it small

- Before adding a decision, a type or a mechanism, look for an existing one it can join.
- No two mechanisms serve one purpose (§2, item 4).
- Nothing outside ARCHITECTURE.md §1's scope is built here (AGENTS.md S4).

## 5. Writing a plan

Before a structural change, state:

1. **The goal**, and what outcome counts as reaching it.
2. **What needs a decision** (§2), as a question for the human.
3. **The changes**, by crate, marked `[NEW]`, `[MODIFY]` or `[DELETE]`.
4. **The check that would fail if the change were wrong.** A verification plan that
   cannot fail is a list of commands.

Decisions go to `docs/adr/`, sequencing to ROADMAP.md. There is no standing plan file.

## 6. Finding out how something works

Never guess at an implementation, a schema, an API, or a library's behaviour.

1. **Run something.** Names, doc comments and documents are claims about the code.
2. **Read the documentation** of a dependency, format or paper before relying on or
   describing it, and cite it with its version; for a dependency, run it as well. Read
   only what AGENTS.md S1 allows. Memory is not a source.
3. **Say where you looked.** A search feels exhaustive from inside its own boundary.
4. **Make a search find something before trusting that it finds nothing.** A wrong
   pattern and an absent thing look the same.
5. **Read whole definitions**: structs, enums and traits entire, and what `lib.rs`
   exports and `Cargo.toml` declares.
6. **Look up, never recall, anything outside the repository** (a release, a crate, a
   format's specification, a paper) **and compute any figure** (a size, a count, a magnitude)
   before writing it.
7. **Compare results as numbers** (by bits, or within a tolerance), never as text: how a
   number is printed differs between languages and formats.

## 7. Retrospective and refactoring

A unit of work that looks back, then tidies up, held before a milestone closes and
whenever a unit leaves duplication behind (ADR-0004); from P5 on, before a milestone
closes only when its checklist lists it (ADR-0033).

1. **The retrospective** sets what happened since the last one against the rules: every
   claim a measurement later corrected, every check found passing without checking, every
   procedure that had to be redone. Each becomes a proposal for a rule, a check or a
   procedure, and the human decides it (AGENTS.md, "Changing the rules"). What is
   decided lands in the ADR on its topic; the rest is not recorded, since history is in
   git.
2. **Refactoring changes no behaviour.** Tests are not changed (TESTING.md T2) and the
   gate passes before and after. A change that alters an output is not refactoring and
   is planned as its own unit.

Enforced by **nothing**; each milestone's checklist in ROADMAP.md lists it.
