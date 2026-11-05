# ADR-0056: A value type is named by an enum, and a field of any stored type is one

- **Status**: Accepted
- **Date**: 2026-11-05

## Context

A survey of designs grown by accretion before 1.0 (ROADMAP.md P9) found a stored value type
named in brodo's public API by its code, a `u32`: `CacheFile::value_type`,
`Region::value_type`, `FormatError::WrongValueType` and `CacheError::NotQuantised`, whose
entry of bytes was code 0. A field read from a `.vdb` was a `VdbField` of `f32` or `f64`.
mestolo, reading a file of any stored type, defined the six types again as an enum of its
own, chose among them by `Stored::TYPE_CODE` by hand, named each by a string, counted what
thawing a grid makes as ADR-0050 states, and copied brodo's private magic numbers to tell
a file's kind.

## Decision

The owner decided, on 2026-11-05, as proposed:

- `field::ValueType`, `#[non_exhaustive]`, names each stored type, with its code
  (`code`, `from_code`) and its name (`name`, `Display`). `Stored::TYPE_CODE` becomes
  `Stored::VALUE_TYPE`, so that the code has one home.
- The accessors and errors name a type by it: `CacheFile::value_type`,
  `Region::value_type`, `WrongValueType { expected, found }`, and `NotQuantised`'s
  `Option<ValueType>`, none for an entry of bytes. `UnknownValueType` keeps the `u32` it
  found. A header naming the type asked for but another size is `Inconsistent("header")`,
  no longer `WrongValueType` naming one type twice.
- `field::AnyField`, `#[non_exhaustive]`, holds a field of any stored type, one variant a
  type, made from each `Field<T>` by `From`. `VdbField` is gone: a `.vdb`'s grid holds an
  `AnyField`, `F32` or `F64`.
- `CacheFile::thaw(i, limit)` thaws a grid into its topology and an `AnyField`, refused as
  `CacheError::ThawTooLarge` if `CacheFile::thaw_size(i)`, ADR-0050's count of what thawing
  makes, passes the limit, the limit last as ADR-0055 places it. The count moves from
  mestolo into brodo unchanged, its nodes still not counted.
- `kind::FileKind::of(head)` tells a frozen grid, a cache file, an archive and a `.vdb`
  by their first eight bytes, whatever features are built; each format's magic number is
  in `kind` and its module reads it from there.
- mestolo holds an `AnyField`, tells a file's kind by `FileKind`, thaws by `thaw` within
  what is left of its limit, and names a type by `ValueType` in its errors.

## Consequences

Every caller of the changed accessors, errors and `VdbField` changes; CHANGELOG.md lists
them. A `match` on `AnyField` or `ValueType` outside brodo ends in a wildcard arm: mestolo's
refuses a type added later as `MestoloError::ValueType`. A count of the nodes thawing makes,
which ADR-0050 left to this review, is not added.
