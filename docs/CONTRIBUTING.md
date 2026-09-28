# Contributing

## Code

1. Fork the repository and create a branch for your change (one change per
   branch, not `master`).
2. Format with **clang-format 11**; CI pins that version and newer ones format
   differently:
   `clang-format -i $(git ls-files "*.cpp" "*.h")`
3. Write commit messages with an imperative first line under 80 characters,
   followed by a blank line and any description.
4. Open a pull request with a short description of the change. For larger
   changes, submit an [RFC](RFC.md) first.

## Issues

Search existing issues first, then
[open an issue](https://github.com/MichaelNguyen5653/Phramer/issues/new/choose)
with steps to reproduce, your Phramer version, your Windows version, and your
monitor setup including display scaling.

Security vulnerabilities should not be reported as issues. See
[SECURITY.md](../SECURITY.md) for how to report those privately.
