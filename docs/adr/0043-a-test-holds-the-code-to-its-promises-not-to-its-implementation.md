# ADR-0043: A test holds the code to its promises, not to its implementation

- **Status**: Accepted
- **Date**: 2026-10-28

## Context

T11 passed only when no mutant was missed, and let a mutant be excluded only "when no test
can tell it from the original"; T3 asked a break larger than a test's tolerance; and no
rule said what a test may assert. A mutant that moved a result within rounding, or chose
another of several broken rules to name, could then only be killed, by tightening a
tolerance or pinning an order, and T2 kept what was tightened. Answering P5's final
mutation run (2026-10-28), tests were written that held a wall's thickness to 10⁻¹³ where
`Thin::least` documents no accuracy and its neighbours use 10⁻⁹, an option's refusal to
its message's wording and to limits no document states, and a Hermite set's refusal to
the first of several broken faces in the order they are checked. A review of every test
found these and a few older ones of the same kind; the rest assert promises or reference
computations.

## Decision

The human decided, on 2026-10-28:

- **T12.** A test asserts what a promise states, or what a reference computation in the
  test gives: ARCHITECTURE.md, docs/formats.md, a public item's documentation, or, for a
  unit test, the documentation of the item it tests. A behaviour worth holding that no
  document states is written into one first, in the same commit. A tolerance is no
  tighter than the promise it checks unless the test says why; where several broken rules
  could be named, the test breaks one, unless the order is a promise. Held by review.
- **T11's equivalence is under the contract.** A missed mutant whose difference breaks a
  promise is killed; one that gives the same result for every input, or differs only
  where a promise leaves the result open, or only on input a documented limit refuses, is
  excluded with the human's approval, its comment ending in that kind and where it is
  written. `mutants.py` checks the kind is there; whether it is true, review.
- **T2's way back.** A test that holds more than a promise is brought to the promise, its
  tolerance loosened or its assertion weakened, under this record rather than a new one.

## Consequences

- A test names, in its documentation, the promise it checks when that is not plain from
  its name, so that review can hold it to T12.
- Some mutants that tests killed before are excluded instead, each with where its
  difference is left open, and a promise too loose to say so is tightened in its
  document, not in a test.
- The tests the review found are brought to their promises.
