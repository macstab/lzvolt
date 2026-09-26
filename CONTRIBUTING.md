# Contributing

Patches welcome. There is one rule that matters more than the rest, and it is
not about style.

## A performance change needs a measurement, not an argument

This codebase has a written record of about twenty changes that were reasoned
from the numbers, looked obviously right, and measured **zero or negative** —
shortening a dependency chain, folding two instructions into one, replacing a
checked read with an unchecked one. They are all in
[docs/MEASUREMENTS.md](docs/MEASUREMENTS.md), with their numbers, because they
are the most useful thing in this repository.

So: if a change is meant to make something faster, the pull request has to say
how much, on what, measured how. "Fewer instructions" is not a measurement.
Neither is "should be faster on modern CPUs".

```sh
make quick            # seven seconds, thumbs up or down, prints the machine
make quick-save       # record the current numbers as the baseline first
```

`make quick` prints a resolution per cell and never claims better than 6%,
because that is what a laptop can actually distinguish. Anything inside that
band is not a result. For numbers worth quoting, `make bench` takes about
twenty minutes.

Three more things the log has taught, the hard way:

- **512-byte cells lie.** They have swung 30% between runs with identical
  code. Measure them in both orders and take at least two runs per revision;
  where the bands overlap, the answer is "nothing", not the average.
- **Watch the control column.** liblz4 is measured in the same loop on the
  same bytes. When it moves, the machine moved, and your column has to be read
  against it rather than against the baseline.
- **Code placement is real.** A cell that cannot possibly be affected by your
  change moving 3% means the binary's layout shifted, and part of your delta
  is that.

## A change to the assembly needs a mutation test

Green tests do not prove the assembly is covered. Corrupt your change
deliberately — one instruction, one constant — and confirm the suite goes
**red**, then revert and confirm it goes green again.

Two real bugs in this repository were found that way and would not have been
found otherwise: an entry bias that read 63 bytes past the input buffer while
still producing correct output, and a text replacement that silently did
nothing, so a hundred green tests were reported for unchanged code.

> Reverting with `mv backup original` restores the file with its old
> timestamp, cargo decides freshness by timestamp, and the next run silently
> reuses the **mutated** build. Revert with an edit, or `touch` the file
> afterwards, and check the test count came back.

## What CI will check

```sh
make check
```

is what runs: the suite with the assembly and without it, clippy with no
warnings, rustdoc with no warnings, `cargo fmt --check`, and the C example
compiled against the real static library under `-Wall -Wextra`. Miri and the
sanitisers run on pull requests; the cross-build matrix runs on top of that.

## Correctness before anything

The decoders' copies are unchecked. The validation in front of them is all
that stands between a malformed stream and memory unsafety, so a change that
touches a bounds check, a margin, or a length needs a test that would fail
without it — see `docs/FORMAT.md` for the eight things a decoder must reject.

If you add a path the fuzzing cannot reach, say so in the pull request.

## Changing the format

Don't, casually. [docs/FORMAT.md](docs/FORMAT.md) is normative and streams are
meant to outlive versions. A format change means a new version of the
specification, new entries in `docs/vectors.txt`, and a good reason.

Adding a *decoder* — a new part line, a new architecture — is not a format
change and needs none of that.

## Style

`cargo fmt`, and clippy clean. Beyond that, match what is there: comments
explain **why**, and where a constant or a shape came from a measurement, the
comment gives the number. A comment that says what the next line does is
noise; one that says why 8192 and not 4096 is the reason the file is
maintainable.

Commit messages are prose — a subject line that says what changed, then
paragraphs explaining why, with the numbers where there are numbers. Look at
`git log` before writing one.

## Legal

Contributions are under Apache-2.0, the same as the project. By opening a pull
request you confirm you have the right to contribute the code under it.
