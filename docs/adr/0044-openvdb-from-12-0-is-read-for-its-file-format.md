# ADR-0044: OpenVDB from 12.0 on is read for its file format, and never copied

- **Status**: Accepted
- **Date**: 2026-10-31

## Context

AGENTS.md S1 forbade reading OpenVDB's source on the ground that it was licensed under
the Mozilla Public License 2.0, copyleft file by file, so that a translation of one of its
files into brodo would bind that file of brodo to MPL-2.0 (ADR-0007's amendment of
2026-10-24). P7 asks whether `.vdb` files can be read and written within S1 (B-006). A
survey of public documentation (2026-10-31) found no specification of the format's bytes:
the one description of them, JangaFX's "VDB: A Deep Dive", was written by reading
OpenVDB's source, and covers one uncompressed grid written, not read; `vdb-rs` 0.6.0
needs `blosc-src`, which compiles C (Rule 12), and states that it follows OpenVDB's
implementation.

The owner found that OpenVDB had been relicensed. Checked on 2026-10-31 from the
project's own documents:

- The release notes (`www.openvdb.org/documentation/doxygen/changes.html`), under 12.0.0
  of 2024-10-31: OpenVDB "is now licensed under the Apache 2.0 license, instead of the
  MPL 2.0 license".
- `LICENSE` at the tags `v12.0.0` (`269300808a4651daa65836214fc20f78e1c0eb7f`) and
  `v14.0.0` (`ecae2bda85744412b0cd47e1b8e1e4e76beb7b88`), the same bytes at both (SHA-256
  `a6cba85b…bf3ff9`): the terms of the Apache License 2.0 as `www.apache.org` serves them,
  word for word apart from white space, without the appendix on applying it.
- `RE-LICENSE_NOTE.txt`, the same at both tags: 28 copyright holders, among them Ken
  Museth, DreamWorks Animation, Side Effects Software, NVIDIA, Digital Domain 3.0, Adobe,
  Walt Disney Animation Studios and the Linux Foundation, relicense all their contributions
  submitted under MPL-2.0 to Apache-2.0, submitted under the Developer Certificate of
  Origin 1.1.
- `README.md` at `v14.0.0`: released under Apache-2.0, the relicensing from MPL-2.0
  completed; contributors' trademarks are not to be used without their permission.

The tags' commits came from `git ls-remote`. The files were fetched by the owner, since
S1's guard blocks the repository's paths. Not checked: whether the tags hold a `NOTICE`
file, and the licence each file states, such as the Blosc that 14.0.0 carries inside the
library. Releases before 12.0 were distributed under MPL-2.0 (ADR-0007).

## Decision

The owner decided, on 2026-10-31:

- OpenVDB's source and documents at a tag from `v12.0.0` on may be read, to learn the
  `.vdb` file format and for nothing else. No earlier version is read. A file that states
  a licence other than Apache-2.0 is read only if that licence is on `deny.toml`'s
  allow-list.
- Its code is neither copied nor translated into brodo. Copying it, which brings
  Apache-2.0's notices and `NOTICE` (§4 of the licence), is decided by an ADR of its own
  if it is ever needed.
- What was read is recorded in PROVENANCE.md, by tag, commit and file, and for what; the
  format is written in brodo's own words in docs/formats.md, as each of brodo's formats
  is (ADR-0037).
- OpenFOAM, and the owner's local list of forbidden sources, are unchanged.
- `scripts/hooks/provenance_guard.py` lets through a path or URL naming OpenVDB only at a
  tag from `v12.0.0` on: a segment `v<major>.<minor>.<patch>`, major 12 or more, among the
  two after the name (`github.com/<org>/openvdb/blob/v12.0.0/…`,
  `raw.githubusercontent.com/<org>/openvdb/v14.0.0/…`), or a directory named
  `openvdb-v<major>.<minor>.<patch>`, the repository's own directories under that tag
  passing with it; a branch, a commit alone or an earlier tag stays blocked, and a path
  that climbs with `..` is judged segment by segment, so that it cannot leave the tagged
  directory. Shown to fire: the guard before this change blocked the four of 15 cases at
  such a tag; after it, those 15 and four paths that climb gave their expected verdicts;
  with the first major version lowered to 11 in a copy, the three at `v11.0.0` passed.

ADR-0005 is unchanged: Apache-2.0 is on `deny.toml`'s allow-list, and no dependency is
added by this record.

## Patents

What Apache-2.0's §3 grants, read from its text (sha256 `cfc7749b…3d30` as served by
`www.apache.org` on 2026-10-31), set out for the owner, who judges it:

- **Covered**: each Contributor grants a perpetual, worldwide, royalty-free licence to
  make, have made, use, offer to sell, sell, import and transfer the Work, under the
  patent claims that Contributor can license and that its Contribution, alone or combined
  with the Work it was submitted to, necessarily infringes. Digital Domain 3.0, the
  applicant of US20100114909A1 (ADR-0007's amendment of 2026-10-25, listed as abandoned),
  is among the Contributors of `RE-LICENSE_NOTE.txt`.
- **Not stated to be covered**:
  - an implementation that is neither the Work nor a Derivative Work: the grant is to
    make and use "the Work", and brodo, which copies none of it, is written apart from it;
  - patents held by anyone who is not a Contributor;
  - a Contributor's claims that its Contributions, alone or with the Work, do not
    necessarily infringe;
  - anyone who brings patent litigation alleging that the Work infringes: the licences
    granted to them for that Work end on the day it is filed.
- Trademarks are not licensed (§6, and the README).

## Consequences

The `.vdb` format can be learnt from its reference implementation, which public
documentation did not allow; B-006 is asked again on that ground. brodo's code stays its
own: reading for the format is held by review alone, as S1 always was, and the guard
distinguishes versions by the tag in a path, so a copy of the source checked out without
its tag in a directory name is blocked rather than allowed. ADR-0007's amendment of
2026-10-24, that OpenVDB was under MPL-2.0 since the Academy Software Foundation took it
on, held for releases before 12.0 only.
