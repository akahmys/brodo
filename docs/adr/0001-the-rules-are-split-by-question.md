# ADR-0001: The rules and the design are split by the question each document answers

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

One document answering many questions is where facts get stated twice and drift apart,
and agents forget rules that sit far from the work they govern. A status file updated
every session, kept apart from the plan, drifted from it.

## Decision

- Each document answers one question, and AGENTS.md lists them and what outranks what.
  The rules are split by the phase of work they govern (planning, coding, testing,
  auditing); the design is in ARCHITECTURE.md, the plan and the status in ROADMAP.md, and
  the reasons in these records.
- The status is ROADMAP.md §1, overwritten at the end of each unit of work, so that
  status and plan are read in one place.
- Documents, code and commit messages are in English. Rules state what enforces them.
- A rule changes only with the human's approval and a record here, in one commit with its
  enforcement.

## Consequences

The design changes only by ADR. `scripts/audit/documents.py` checks the links, the
language, the tense of the present-tense documents and this index; the `commit-msg` hook
requires an ADR for a commit that changes a rule.
