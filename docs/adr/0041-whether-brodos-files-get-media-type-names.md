# ADR-0041: Whether brodo's files get media type names

- **Status**: Accepted
- **Date**: 2026-10-25

## Context

Users passing cache files and Hermite sets between processes may want a name for them in a
registry, a manifest or a protocol. RFC 6838 (2013), read on 2026-10-25: the vendor tree,
`vnd.`, is for publicly available products, and "a registration may be placed in the
vendor tree by anyone who needs to interchange files", reviewed by IANA's experts; the
personal tree, `prs.`, is for experimental or uncommercial products; the `x.` tree cannot
be registered and its use is "strongly discouraged"; a structured-syntax suffix is used
only for a syntax the type employs.

## Decision

The owner chose, on 2026-10-25, the recommended option, (2):

- (1) **No name**, as now; files are known by their magic numbers.
- (2) **Names stated, not registered**: `application/vnd.brodo.cache` for a cache file and
  `application/vnd.brodo.hermite` for a cache file holding a Hermite set, written in the
  format document (ADR-0037) with each format's version as a parameter, registered with
  IANA only if publishing (ROADMAP.md §4) makes it worth doing.
- (3) **Names registered now** in the vendor tree.

*Recommended: (2)*: users get a stable name to write, at no cost now; registering waits for
the repository to be public, as the vendor tree is for public products.

## Consequences

Under (2) or (3), a Hermite set, being a cache file with keyed grids (ADR-0032), is named
apart only as a hint: a reader still checks it as a cache file first.
