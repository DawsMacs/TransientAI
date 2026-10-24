# Golden outputs

What Transient produces on the reference platform (Linux x86_64, glibc), kept so that every other platform can be compared with
it byte for byte. `check.sh` generates the same outputs on the machine running it and diffs them against these files. See
the section "Running on other platforms" in `baby/README.md` for the reasoning and the results.

| File | What it is |
|---|---|
| `predictions-exam60-seed1.tsv` | The baby's answers to the 60-unit entry exam (`baby exam write --units 60 --seed 1`, then `exam run --out`): 4,832 questions. |
| `replies-adapt-seed1.tsv` | Its replies to the adaptive stream with a clean teacher (`exam adapt-write --seed 1`, then `exam adapt-run --out`): 787 questions, each with the stated rate. |
| `scorecard-seed1.txt` | The `baby school` scorecard table, minus the one line that carries a timing. |
| `PROVENANCE.txt` | Which compiler and machine produced them, and how many runs agreed. Informational; not compared. |

Only deterministic outputs are compared. Timings, memory, joules and meter warnings are never part of a golden.

## Reading a result

- **OK** on every file: this platform's answers are byte-identical to the reference.
- **DIFFERS**: show the difference and say so; do not widen anything or adopt the new output. In CI this **fails the job on the reference row** (Linux x86_64 glibc) and is only reported on the others. It means either a platform
  difference (for example a math-library last bit that flipped a decision) or a logic change that was not followed by an update.
- **NOT REPRODUCIBLE** (exit 3): the runs on one machine disagree with each other, so a comparison would prove nothing. The first
  suspect is hash-map iteration order (`baby/README.md`, "Running on other platforms", latent hazards).

## Updating the goldens

Only on purpose, when a change to the logic is meant to change the answers, and the commit says why:

```bash
golden/check.sh update
```

Run it on the reference platform (Linux x86_64). Goldens regenerated elsewhere would make that platform the reference.

## Not covered here

The wrong-teacher stream (about 20 to 30 minutes per run) is not a golden yet; it belongs on a schedule, not on every push.
