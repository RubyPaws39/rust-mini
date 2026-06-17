# Security Policy

Rust Mini is an educational interpreter project.

## Supported Versions

Current development happens on `main`.

## Reporting Security Issues

Please do not publish exploit details first.

Open a private report to the maintainer if GitHub security advisories are enabled, or contact the maintainer directly.

Include:

- affected command or file
- reproduction steps
- expected impact
- OS details

## Current Security Notes

Rust Mini programs can use host helpers like file I/O. Treat `.rmini` scripts like code, not plain documents.

Do not run untrusted `.rmini` files on important machines.
