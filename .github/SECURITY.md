# Security Policy

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on this repository. Expect a response within 7 days. A confirmed issue is fixed in a patch release.

## Scope

`seqcell` is a small `no_std` library with no allocation and no I/O. It contains `unsafe` code for its atomic payload copy, and a defect there could let a reader return a value no writer stored. A report that a sequence of safe calls makes a reader return a mixed value, a value that is not a valid `T`, or a Miri error is in scope. A writer that stalls mid-store and leaves `load` spinning is documented behaviour, bounded by `try_load`.
