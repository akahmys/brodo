# Security

## Reporting a vulnerability

Report a vulnerability privately, through GitHub's private vulnerability reporting:
[open a report](https://github.com/akahmys/brodo/security/advisories/new) from the
repository's Security tab. Do not report one in a public issue.

Say which crate and version, and give the input that shows it, or the steps that make it,
and what happened. One person maintains brodo: a report is answered once it is read, and
no time is promised.

## What is in scope

brodo, colino and mestolo read bytes they cannot trust: brodo's cache files, archives,
frozen grids and Hermite data, `.vdb` files, and STL, OBJ and 3MF meshes. Reading any of
them is to return a value or an error. A report is in scope when such bytes make one of
the three crates

- panic, or abort;
- reach undefined behaviour;
- read or write out of bounds;
- run without end, or use memory or time out of all proportion to the input.

Out of scope: [`CacheFile::map`](crates/brodo/src/mapping.rs) on a file that is changed or
truncated while it is mapped, which breaks the condition its `# Safety` section states,
and a program's own misuse of an API that the API documents.

## Supported versions

The latest release alone is fixed. A fix is released as a PATCH of it (TESTING.md V2);
an earlier MAJOR or MINOR receives none.

## How a fix is disclosed

The fix is made in private, in the report's advisory, and released. The advisory is then
published on GitHub, with a CVE that GitHub assigns, and the vulnerability is filed with
the [RustSec advisory database](https://rustsec.org/), so that `cargo audit` reports the
affected versions. The reporter is credited, unless they ask not to be.
