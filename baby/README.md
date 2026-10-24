# baby

Transient's first working unit, written from scratch in Rust with **zero dependencies**. It started on 2026-10-15 as a tiny network
that read a short query and said one word, and has become something else: a baby that is **taught units of lessons, works out for
itself how to learn each one, answers by the way that worked, says how it knew, and refuses when it should**. The first network is
still here (`baby train`), because every later idea was tested against it.

## Try the baby (the unified one)

```
cd ~/Transient
cargo build --release --offline
./target/release/baby school                   # teach it: 13 units of lessons, about 12 seconds; prints how it learned each one
./target/release/baby ask "what is two times four"
./target/release/baby ask "how far apart are two and nine"
./target/release/baby ask "the capital of peru"
./target/release/baby ask "what is the code for five"      # it was never taught this one: it says so
./target/release/baby teach "capital of atlantis" poseidonis   # tell it something new; it keeps it in a file you can read
./target/release/baby learn "what is two multiplied by three" six --unit times   # keep learning AFTER school (a new phrasing, or a whole new unit)
./target/release/baby effort                   # what answering costs: the slow way vs the baby's way
./target/release/baby cost                     # what LEARNING costs: time and memory for teaching it, a new lesson, a new kind of lesson
./target/release/baby storage                  # what it keeps: lessons vs what it learned from them, per unit
./target/release/baby sleep --compact          # forget what is already explained (about half the file), keep every promise
./target/release/baby grade                    # re-run the honest scorecard on a saved school
./target/release/baby discover --out runs/discovered.school   # hand it a heap of UNLABELED lessons; it finds the units itself
./target/release/baby ask --school runs/discovered.school "three minus five"   # ask the baby that organized itself
```

Every answer comes with how the baby knew (`worked out by an expression I built, (first x second), 100% on facts I held back` /
`recalled, not worked out`), and every refusal with why (`I haven't met the word...`, `I only memorize this kind of thing and
nothing I was taught is close enough`, `no kind of lesson I was taught is like this question`).

The older experiments, each of which has its own section below:

```
cargo test --offline                 # 61 tests, including numerical checks of the hand-written backprop
./target/release/baby kinds          # the nine-unit mechanism comparison behind the school (about 30 s)
./target/release/baby life           # five ways of learning, charged in estimated joules
./target/release/baby curious        # a baby that chooses what to ask a teacher
./target/release/baby rules          # sums as a rule, not a table
./target/release/baby know           # does it know what it doesn't know? three refusal signals compared
./target/release/baby train          # the original single network (--reader concat|bag|bigram|letters)
./target/release/baby energy         # what one question and one learning step cost, counted
```

## Running on other platforms: five CI platforms, one set of answers (2026-10-24)

The question: does Transient give the same answers off Linux/x86_64, and if not, by how much and where? The rule for the work: **the logic files stay as they are; every platform is compared with what Linux/x86_64 produces; a difference is reported, never absorbed; a platform adds a row to a "tested on" table and cannot move a recorded bar.** Done through branches and pull requests (#1, #2, #3), so `main` never carried an experiment.

**What exists**
- `.github/workflows/ci.yml`: a matrix of five rows, each building without network, running the tests, the scorecard WRONG gate, the math fingerprint, the accuracy audit and the golden check. Rows: Linux x86_64 glibc (`ubuntu-24.04`, the reference), Linux aarch64 glibc, Linux x86_64 musl (same CPU, different C library), macOS arm64 (`macos-15`), Windows x86_64 (`windows-2022`). Actions are pinned by commit, permissions are read-only, no secrets. Each row records `rustc -Vv` and the OS, because no runner label is a true pin (images update underneath it; `windows-2025` itself moved to Visual Studio 2026 in June 2026).
- `baby/examples/mathprint.rs` (+ `mathprint.expected`): a **fingerprint** of the platform's math library over the 24 platform-dependent calls (`exp`, `ln`, `log2`, `tanh`, `powi`; `+ - * /` and `sqrt` are exact everywhere, and the source never calls `mul_add`) on the input ranges the code reaches: 64-bit hash of every result's bits plus one hash per 65,536 inputs. `cargo run --release --offline --example mathprint` prints it; `-- dump NAME FROM COUNT` lists values; `-- audit` compares every result with the f64 result rounded to f32, so a difference gets a size. It also prints each literal-argument call folded by the compiler and forced at run time.
- `golden/`: the exam predictions (4,832), the clean adaptive replies (787, with stated rates) and the scorecard table, plus `golden/check.sh` (generates them three times, requires the runs to agree with each other, then compares with the goldens; exit 0 match, 1 differ, 2 could not run, 3 runs disagree; `golden/check.sh update` to regenerate on purpose) and `golden/README.md`. Never timings, memory, joules or meter warnings. `.gitattributes` keeps golden bytes untouched on Windows.
- `baby/src/platform.rs`: the questions Transient asks the operating system. Where the system cannot say, memory reports `n/a` instead of `0 MB`, and the energy message no longer tells anyone to `chmod` a RAPL file that does not exist. No `unsafe`, no `cfg`, no dependency. Test-only change: the Python pipe test skips unless `python3` actually runs (a Windows Store stub starts and fails).

**Results (CI, 2026-10-24; 149 tests pass on all five rows; the scorecard WRONG column 0.0 % on all five)**

| Row | `ln`/`log2` (decision path) | `exp` | `tanh` | `powi` | goldens (exam, replies, scorecard) |
|---|---|---|---|---|---|
| Linux x86_64 glibc (reference) | reference | reference | reference | reference | match |
| Linux aarch64 glibc | same bits | same | differs | same | **match** |
| Linux x86_64 musl | same bits | same | differs | same | **match** |
| macOS arm64 | differs | differs | differs | same | **match** |
| Windows x86_64 | differs | differs | differs | differs | **match** |

- **The answers are byte-identical on all five platforms**, including macOS and Windows where `ln` and `log2` differ in the last bit. Cross-check: the pre-cost-cut commit `3a47fcb`, built separately, gives the same exam predictions and adaptive replies.
- **The math differences are one last-bit step.** Against the correctly rounded result, every `ln`, `log2` and `exp` input on every row is at most one step off, and 99.1 % or more are exact (word-weight `ln` on the reference: 0.862 % one step off; macOS 0.003 %; Windows 0.264 %; `log2`: 0.009 % / 0 / 0.626 %). **The reference library is not the truth**: glibc is the least accurate of the three families for `ln` and `log2`; macOS is almost exactly correctly rounded for them but worse for `exp`; Windows is best for `exp` and worst for `log2`. Two libraries that are each within one step can disagree with each other by two. Exactly representable results (`log2(2^k) = k`, `ln(1) = 0`, `exp(0) = 1`, `tanh(0) = 0`) were exact everywhere, so ties built on them cannot break differently.
- `tanh` (old network only, not the decision path) differs on aarch64 and musl; on musl the compiler-folded `tanh(0.5)` (`3eec9a9f`, the build machine's glibc) differs from the run-time value (`3eec9a9e`). No other literal-argument call showed folding.
- `powi` is by design not correctly rounded (repeated multiplication): in `0.999^t` it is up to 12,119 steps off on Linux and macOS (Windows within one). It sits in the old network's optimizer correction, not on the decision path; its effect on any result is not measured.
- `ln` and `log2` do sit on the honesty guards' decision path (`school.rs` routing scores; `program.rs` believability test comparing the lessons' bits with the search's bits), so a last-bit difference could in principle flip a near-tie. On everything tested it did not.

**Corrections made on the way (kept so they are not repeated).** I counted 32 transcendental calls and "about 13" on the decision path; the platform-dependent count is **24** (the other 9 are exact `sqrt`) and the decision path has **11 calls on 10 lines**. A green CI check does not mean bit-identical math: the fingerprint step only warns. `windows-2025` was not a pin. Fused multiply-add is not a risk here; the math library is. The first Linux goldens could not include timing, memory or joule lines.

**Latent hazards found by reading the source (not fixed: they are logic changes).** Rust randomizes hash-map iteration order per run, and two places depend on it: `quantity.rs` `fit_ridge_dual` adds into an f64 matrix while looping over a `HashMap` (last bits may differ between runs), and `Quantity::terms()` feeds `learn_constant` in hash order into a 6-slot first-in-first-out list that is saved. Eight runs of `baby school` and six of `baby scale` were byte-identical, and every golden run was, so neither has been triggered; it is evidence, not proof, and it is why `golden/check.sh` requires the runs on one machine to agree before it compares anything. Sorting the keys would be a logic change needing its own commit and a re-golden.

**Decisions that stay with Daws.**
1. **Deterministic in-repo math** (replace the 11 decision-path `ln` and `log2` calls with a small implementation built from the exact operations). It would change Linux's numbers, so it would be a new labeled version with the bars re-earned, not a port. Not needed so far: the differences are one step and no answer changed.
2. **WASI** as a reference build whose math should be host-independent (a wasm binary carries its own math code; the only floating-point non-determinism in the WebAssembly specification is NaN payload bits). Unverified: the exact code path for `wasm32-wasip1`, and whether two engines agree. It would need its own goldens (it likely differs from glibc) and could not run `scale --sweep`, `adapt-drive` or the Python test (process spawning). If it ever became the reference, the bars would have to be re-earned on it.
3. **Hash-order fix** (above).
4. **Real peak memory off Linux** needs small `unsafe` blocks and has different units per system (bytes on macOS, kilobytes on Linux and FreeBSD). Not proposed.
5. A recorded bar can never be changed by a port.

**Wording for anything public:** "CI-tested on GitHub's runners" (VMs running the real operating systems, not emulation). Joules are Linux with RAPL only (bare metal, readable by the user); the counted-operations estimate must not be quoted as joules (it was 34-116x off). Not covered: other OS versions and hardware (Intel macOS, Windows on ARM, i686, s390x, riscv64, the BSDs), bare-metal energy, real-world sandboxes. Rust's own tiers bound what to promise: tier 1 for x86_64/aarch64/i686 Linux, macOS arm64 and Windows MSVC; tier 2 with host tools for musl, s390x, riscv64, FreeBSD, NetBSD and Intel macOS; tier 3 for OpenBSD and DragonFly.

**Done 2026-10-24:** a golden difference now fails the job on the reference row only (after the golden step had run twice there; "runs disagree" stays a warning, because of hash order), proved by a throwaway pull request with an altered golden. **Next.** A scheduled wrong-teacher golden (about 20-30 minutes, skipped on emulated rows; GitHub may disable scheduled workflows on inactive public repositories after about 60 days: check it still fires); a WASI row; the BSD rows (FreeBSD, NetBSD, OpenBSD, DragonFly via `cross-platform-actions`), i686, and s390x and riscv64 under QEMU; widen the fingerprint's word-weight range (it covers 1,200 x 1,200 count pairs; the adaptive exam teaches 3,048 lessons). **Not verified:** the BSD and QEMU runs, OpenBSD and NetBSD Rust package versions against our minimum, RAPL's absence in VMs and containers (secondary sources only), the 32-bit casts, and mixed-separator paths on Windows (CI passed, which suggests they work).

## Cutting the cost of adapting: 3.7x per told answer, replies byte-identical (2026-10-23)

The transformer comparison made the baby's adapting the number to beat (0.22 J per told answer). `BABY_PROF=1 baby exam adapt-run` on the clean stream (seed 1, ~100 s) said where it went: **periodic re-decisions** (`relearn_unit` called from `learn_wondered`) 56 calls, 51 s; **`refit_in_place`** 274 calls at 110 ms, 30 s; **library review** 10 s. `RELEARN_DEBUG=1` (new) prints each periodic re-decision with the unit, its facts and what it chose before and after. The finding: **all 56 re-decisions ended where they started** (24 Program to Program, 13 Rule to Rule, 12 Remember to Remember, 2 Select to Select, plus 5 surprised Program); six exact-program units at 25 and 50 facts cost 4-7 s each.

**The change.** A settled exact program (no rivals, not partial, reliable) that still says every one of its lessons exactly, the new one included, is neither refitted (`learn`) nor re-decided every 25 lessons (`learn_wondered`): `School::settled_program_explaining_all`. The argument: a smaller program that fits all the lessons would also have fitted the earlier ones and been chosen; a new vocabulary piece is handled by library review; a contradiction makes the program stop explaining everything, and the full path runs as before. Only programs: a rule's ridge fit does change a little with every extra lesson, so rules and selections keep their refits.

**Checked, not assumed.** The stream's replies (`--out`) are **byte-identical** between the old and new binary on five streams: clean seeds 1, 2, 3, and the wrong-teacher tier at 10 % (seed 2) and 20 % (seed 3). The whole regression battery reads exactly as before (persistent teacher 87.40 / 74.89 / 81.96 % right and 39 / 52 / 45 wrong, all repeats, 0 elsewhere, 0 traps; careful deceptive 331 / 478 / 684 with WRONG 0; hidden 64/64, stage-2 32/36, WRONG 0; scale and noise 0.00; 100 % at 20 and 200 units). A new test (`a_confirming_answer_extends_a_settled_program_without_refitting_it`) pins what the shortcut must still do: keep the confirmation and extend the reach, and stop applying the moment a lesson is contradicted.

**Result, paired and serial on a quiet machine (no meter warning), two rounds:**

| clean adaptive stream, seed 1 | before | after |
|---|---|---|
| asking + learning the 787 questions | 179.1 / 174.9 J (98.5 s) | **48.0 / 47.6 J** (24.2 s) |
| per question (every question is told its answer) | 228 / 222 mJ | **61 / 60 mJ** |
| teaching the 3,048 lessons | 37.7 / 35.1 J | 33.1 / 33.6 J (unchanged within noise) |
| teaching + stream | 216.8 / 210.0 J | **81.1 / 81.2 J** |
| right answers per joule (facts + right labeled guesses), overall | 3.2 / 3.3 | **8.5** |

Against the transformer (previous section): the baby's 60 mJ per told answer is now within ~2x of the transformer's replay fine-tuning (27-34 mJ) and ~20-40x above its told-answer-only fine-tuning (1.6-3 mJ), which does not learn. **What remains** (`BABY_PROF`): about 13 s of periodic re-decisions for rules, selections and memorize-only units (33 calls, all of the kind that ended where they started in the trace), 10 s of full re-decisions inside `learn` (10 calls at ~1 s), and 12 s of initial `learn_items`. The same kind of argument for a rule (skip when the fit still explains every lesson) would change its coefficients, so it needs its own byte-identity test; a geometric back-off for memorize-only units (re-decide at 10, 20, 40, 80 facts) delays noticing a unit that becomes learnable. Neither is done.

**A belief corrected.** The handover said the remaining cost was "`review_programs` (4 rounds of ~11 s), `refit_in_place` and the 25-lesson re-decisions". After this change `review_programs` is 0.02 s: it was not a cost of its own but a consequence of the needless re-decisions (each grew the library and triggered a review). Fixing the cause removed it.

### Second cut: a re-decision that only repeats the one just made; and a back-off I rejected (2026-10-23, later)

With the first cut in, `BABY_PROF` and a new trace line (`RELEARN_DEBUG=1` prints `LEARN-RELEARN` for the re-decisions inside `learn`) showed what was left: 33 periodic re-decisions (12.8 s), of which **all the expensive ones were memorize-only units** (the unexplainable `code_*`, `xorfive`, `prime`) at 20 and 30 facts, about 1 s each, and 10 `learn:relearn_unit` calls of ~1 s on the same units at the same boundaries. The same units were re-decided **twice per told answer** at those boundaries: `learn` re-decides a memorize-only unit every ten facts, and `learn_wondered` then re-decided it again from the same lessons and library. `School.just_relearned` records (unit, lesson count, library) when `learn` has just re-decided a unit and left its state as it was (no repair followed); `learn_wondered` skips the second, identical call. Result: periodic re-decisions 12.8 s -> 2.8 s, replies byte-identical. A test (`a_unit_that_only_memorizes_is_not_re_decided_twice_for_one_told_answer`) compares the public path against the spelled-out old one (learn, then re-decide).

**Rejected by the evidence: a back-off for memorize-only units** (re-decide at 10, 20, 40, 80 facts instead of every ten). On the clean stream and three others it was byte-identical and the whole battery read as before; learning from told answers fell from 14.8 s to 8.2 s. But on the **wrong-teacher stream at 20 % (seed 3) it changed two replies**: a `wraptriple` question went from a labeled guess to a stated fact (right), and another from a refusal to a stated fact that was **wrong** (a repeat of a wrongly told fact: 29 repeats against 28; elsewhere still 0). A unit that a wrong lesson has knocked into memorize-only is exactly one that the every-ten-facts look gets back onto its program sooner; the back-off leaves it memorizing longer, and memory then states what it was told. My rule for these cuts was "any changed answer, drop it", so it is dropped (the build with only the duplicate skip was checked on that very stream: identical). *The byte-identity check across a wrong-teacher stream caught what the clean streams and the whole battery could not.*

**Final numbers**, paired and serial on a quiet machine (one run of four was discarded because a compile overlapped it: the meter flagged 62 % other processes and read 560 J):

| clean adaptive stream, seed 1 | original | after cut 1 | after cut 2 |
|---|---|---|---|
| asking + learning 787 questions | 175-195 J | 48 J | **28.5 J** (14.4 s) |
| per told answer | 222-248 mJ | 61 mJ | **36 mJ** |
| teaching + stream | 210-233 J | 81 J | **65 J** |
| right answers per joule | 3.0-3.3 | 8.5 | **10.6** |

Per told answer that is **6-7x cheaper than before**, and the same order as the transformer's replay fine-tuning (27-34 mJ) while the baby keeps 0 wrong facts and the transformer does not learn. What is left in the stream: 10 re-decisions inside `learn` on memorize-only units (~10 s of the 14 s, each a cross-validation of every mechanism over lessons nothing can explain), 2.8 s of periodic ones, 12 s of initial `learn_items` (teaching).

## The adaptive exam, put to the matched transformer: what it costs to learn from being told (2026-10-23)

`baby exam adapt-transformer DIR [--steps N --replay N]` teaches the plain transformer the adaptive exam's lessons once (3,048 lessons), then puts a copy of it the 787-question stream three ways: **frozen** (never learns), **fine-tune on the told answer only** (3 passes, Adam, lr 0.003), and **fine-tune on the told answer plus 16 replayed old lessons**. Spare answer slots are reserved so a new answer can be learned at all. Each is scored always-answering and refusing-below-0.9, by the same stream judge as the baby, with the same meter. Clean stream, seeds 1 / 2 / 3, run serially after the baby's (`adapt-run`) in the same session.

| | baby | transformer frozen | + fine-tune, told only | + fine-tune + replay 16 |
|---|---|---|---|---|
| facts right | 64.2 / 65.2 / 64.4 % | 19.1 / 18.8 / 19.4 % | 10.7 / 11.7 / 10.9 % | 15.9 / 14.2 / 15.6 % |
| right *labeled guesses* | 23.8 / 23.0 / 23.6 % | (none) | (none) | (none) |
| **wrong facts** | **0 / 0 / 0** | 80.9 / 81.2 / 80.6 % | 89.3 / 88.3 / 89.1 % | 84.1 / 85.8 / 84.4 % |
| climb, first third -> last third | 86.7 -> 88.5, 86.3 -> 91.2, 87.5 -> 90.1 % | 18.6 -> 21.8, 22.4 -> 16.8, 18.6 -> 20.6 % | 10.3 -> 10.7, 14.1 -> 8.8, 8.4 -> 13.0 % | 14.1 -> 19.8, 16.3 -> 13.0, 16.3 -> 13.7 % |
| initial teaching energy | 39.4 / 41.8 / 41.3 J | 84.9 / 55.6 / 97.7 J | same model | same model |
| the stream (asking + learning) | **174 / 178 / 183 J** (221-233 mJ per question) | 0.07-0.08 J | 2.0 / 2.4 / 2.5 J (2.6-3.2 mJ per told answer) | 22.1 / 21.6 / 21.5 J (27-28 mJ per told answer) |

Refusing below 0.9 does not change the picture: the frozen model still states 33-37 % wrong facts and refuses 47-52 %; the fine-tuned ones refuse 80-91 % and are still wrong in 4-12 % of all answers. Gentler settings on seed 1 (1 pass; with 16 or 64 replayed lessons) gave 16.4 % / 18.9 % / 16.4 % facts right: no setting I tried beat frozen.

**What this says, and does not say.**
- **The transformer does not adapt here.** Fine-tuning on a told answer never improved it over being frozen, and mostly hurt it (the forgetting the earlier comparison found at 20 new facts shows up again as a loss on the stream's other questions). Rule-changing kinds are exactly what a classifier over answer words cannot learn from one example at a time: it can only emit answers it has been trained on, so a number it was never taught is unreachable until told.
- **The baby's adapting is the expensive side.** Its stream costs 174-183 J against the transformer's 0.07-27 J; per told answer that is 0.22 J against 1.6-34 mJ, **6.5x to 140x more**. Counting teaching too, the baby's whole adaptive run is 213-225 J and the frozen transformer's 56-98 J (the transformer pays almost all of that once, up front). In right answers per joule (the baby counting its right labeled guesses, which the transformer has none of) the two are in the same range, baby 3.1-3.2 against transformer 1.6-2.7, but the transformer's right answers come with 80 % wrong facts and the baby's with none. **This is the measured bar for Novice's "genuine AL": the same honesty and learning at a fraction of 0.22 J per told answer.**
- Caveats: joules are upper bounds on a busy machine (other processes up to ~50 % of the busy time); the transformer here is one small untuned classifier (the update settings tried were few: 1 or 3 passes, 0 / 16 / 64 replayed lessons, lr fixed), a bigger or better-tuned one, or a model that can generate numbers rather than choose among taught answers, was not run; the stream is the invention battery's kinds (a third of them unexplainable or regime-changing by design), so absolute accuracies are low for everyone but the baby; only the clean stream was run (the wrong-teacher tier on the transformer is untried).

**A tooling slip worth recording.** My first tests for the new commands took 13 and 36 s each: the real meter samples the machine's idle draw for several seconds before and after every run, and RAPL is readable here, so any test that passes `Meter::new()` pays that. Tests now use `Meter::off()` (suite back to ~20 s). The first version of the report also printed "J J" and rounded 0.0016 J to "0.00 J".

## The exam, put to the matched transformer: capability per joule (2026-10-23; Vision Entry 7)

`baby exam transformer DIR [--d N --layers N --epochs N --threshold P]` teaches the repo's plain attention transformer (`transformer.rs`, 2 layers, d = 32, trained to fit its lessons, up to 60 epochs) the static exam's lessons and puts it the same questions, judged by the same judge and measured by the same meter as `baby exam run`. A transformer cannot say "I do not know" by itself, so one trained model is scored two ways: **always answering** (every answer a fact) and **refusing below its own probability 0.9** (a conventional cut; mine, untuned). 60 units, 2,520 lessons, 4,832 questions; seeds 1, 2, 3.

| | baby (seeds 1 / 2 / 3) | transformer, always answers | transformer, cut at 0.9 |
|---|---|---|---|
| taught / reworded, facts right | 100 / 100 % | 100 / 100 % | 100 / 100 % |
| never taught, facts right | 94.9 / 94.9 / 83.7 % (the rest right guesses) | 97.7 / 94.4 / 97.7 % | 93.4 / 87.5 / 90.6 % |
| beyond the taught range | 100 % right *labelled guesses*, 0 facts | **17 % right, 82 % wrong facts** | 14 % right, 44-53 % wrong facts, 33-41 % refused |
| wrong facts stated | **0 / 0 / 0** | 2,803 / 2,772 / 2,786 | 1,497 / 1,787 / 1,710 |
| traps answered (of 140) | **0 / 0 / 0** | 140 / 140 / 140 | 64 / 68 / 60 |
| teaching energy | 1.88 / 1.02 / 0.54 J | 21.6 / 24.9 / 10.9 J | same model |
| answering, per question | 58 / 67 / 55 uJ | 158 / 145 / 135 uJ | same model |

The transformer teaches itself in 10-15 epochs (39,000 parameters) and fits what it was taught perfectly. Per question it is 2.2-2.7x the baby's answering cost; teaching is **11-24x** the baby's (paired in the same session, same load; the baby's teaching window is 0.34 s, so its joules swing from 0.5 to 1.9 J between seeds while its answers do not change). The comparison that matters is the honesty column: a cut on the transformer's own confidence still states 1,500-1,800 wrong facts and answers 43-49 % of the traps, because its confidence is high exactly where it has no basis (the same finding as the earlier comparison, now on the exam any successor is judged by).

**Caveats, stated plainly.** (1) Joules are the processor package above idle with other processes using up to ~49 % of the busy time during these runs: upper bounds, +-30 %. (2) "Right answers per joule" is not comparable across the two without reading the table: the baby's 4,692 right answers include 3,396 right *labelled guesses* beyond the taught range, the transformer's 1,889 are stated facts, and 2,943 of its facts were wrong. By the honest count (right minus stated wrong) the transformer's always-answering run is at 0 per joule and the cut run at 0-10 per joule. (3) One small plain transformer, trained once with no search; the earlier comparison's 24-setting PyTorch search did not change the unseen-question picture, but a larger or tuned model was not run here and the pretrained-model comparison still waits for the GPU. (4) The baby's never-taught coverage depends on the world: 83.7 % facts on seed 3 (16.3 % right guesses) against 94.9 % on seeds 1 and 2; the handover's 94.9 % bar is seed 1/2, not an average. (5) The exam is the baby's home ground (synthetic, tidy, number words); the transformer is not at a disadvantage in reading it, but the questions beyond the taught range need answers it has never seen as classes, which is exactly the point of those questions. (6) Not run: the adaptive tier for the transformer (what it costs to learn from a told answer; the earlier comparison measured fine-tuning at 125-227 mJ and forgetful, replay at 1.2-3.9 J, against the baby's ~60 mJ for 20 facts); that is the natural next step.

## The exam's wrong-teacher tier, and the hole it found on its first run (2026-10-23)

**What was added.** `baby exam adapt-write DIR --wrong P --repeat F`. The adaptive stream's truths are told wrongly for a share `P` of the *facts*, consistently: every time that fact comes up, in any phrasing or as an exact re-ask, the teacher says the same wrong answer (the true answer plus 1-9). `--repeat F` re-asks that share of the stream later, so a believed mistake can show itself. `stream.tsv` gains two columns, `TOLD` and `FACT`; the `TOLD` protocol line carries the told answer, so a system cannot tell. Answers are judged against the truth. The judge splits each wrong fact in two: a **repeat** (that fact was wrongly told earlier in the stream: the system believed its teacher) and **elsewhere** (a fact never wrongly told: the teacher's mistakes damaged something). **Elsewhere must be 0**; repeats are reported, not failed. This is open problem 1 of the handover.

**First run, before any fix (seeds 1-3, 30 % re-asks).** The clean stream was fine (0 wrong facts on all three seeds). The wrong-teacher stream was not:

| wrong teacher | seed | facts right | wrong facts | repeats | **elsewhere** |
|---|---|---|---|---|---|
| 10 % | 1 / 2 / 3 | 61.4 / 65.0 / 63.0 % | 14 / 15 / 11 | 14 / 14 / 10 | 0 / **1** / **1** |
| 20 % | 1 / 2 / 3 | 51.4 / 56.7 / 52.2 % | 39 / 26 / 28 | 39 / 25 / 28 | 0 / **1** / 0 |

**What the one-unit trace showed** (`STREAM_UNIT=<unit>` prints each question to that unit with how it answered and what it held). The tens-digit kind held the correct `first div 10` until the teacher wrongly said `28 -> 8` (true: 2). Right after that lesson the unit held `(first mod 10) apart (((first div 10) + (first mod 10)) mod 10)`, which says 8 at 28 and *is* the tens digit wherever the two digits sum to less than ten. It fits every lesson including the wrong one, with no exception to pay for, so the fresh decision preferred it; it replaced a correct law, was not marked as a repair (so the replaced-law veto of last session never applied), and 19 was stated as fact: 9 (true: 1). A second case (`modthree`) looked the same. This is the ambiguity family again, now entered through a wrong teacher: **one wrong answer can pose as support for a rival law.**

**The fix.** `School::rebuild_policy`, used by both `relearn_unit` and `refit_in_place`: an established exact program (no rivals, reliable, >= 12 facts) that a fresh exact fit would replace, where the old law contradicts one to three lessons, is replaced only **as a repair**: the contradicting lessons are marked unproven (they do not vouch for their neighbors) and the replaced law stays a competing explanation (`repair_veto`), so where the two part ways the new law only guesses until two later lessons separate them. The agreement guard of `repair_regime` was factored out (`rebuilt_agrees_with_old`) and is still used there.

**What I tried first and was wrong about.** My first version *refused* the rebuild when it disagreed with the old law inside the range the other lessons cover. It broke an existing test (`a_unit_with_one_explanation_checks_it...`): when the single contradiction is the *truth* (told 9 -> 9 against a coincidental `first mod 9`), refusing keeps a wrong law. The data cannot tell that case from this one (the old law was right here, wrong there), so the symmetric answer is the one the repair step already had: adopt, keep the rival, state only where both agree. A second consequence needed a flag: a program adopted this way is `rebuilt`, and when the library later grows and something else fits equally well it is re-examined (`reconsider`); a repair built from the numbers a contradiction names stays sticky as before. Two weak things I wrote and replaced: a unit test that passed *with and without* the fix (it fell to memory in its small fixture; the replacement offers the coincidence as the fresh fit by hand and fails without the fix, checked), and a one-unit replay switch (`STREAM_ONLY`) that taught the unit alone: the tens digit needs pieces other units contributed to the library, so it never reproduced (removed). I have **not** established whether the swap went through `relearn_unit` or `refit_in_place` in the exam run (the unit had ~40 lessons, so either is possible); the policy sits in both.

**After the fix** (same streams, same seeds): elsewhere **0 / 0 / 0** at 10 % and **0 / 0 / 0** at 20 %; facts right 61.4 / 64.9 / 63.0 % (10 %) and 51.4 / 56.6 / 52.2 % (20 %); repeats unchanged (14 / 14 / 10 and 39 / 25 / 28); the clean stream byte-identical in totals (64.2 / 65.2 / 64.4 % facts right, WRONG 0). The tens-digit trace now reads `repair (1, "program")` after the wrong lesson and 25 is a guess. Regression battery (all rerun): persistent teacher 87.40 / 74.89 / 81.96 % right, 39 / 52 / 45 wrong, all repeats, 0 elsewhere, 0 traps; careful deceptive 331 / 149 / 0 (limit 1), 478 / 2 / 0 (limit 2), 684 / 36 / 0 (3 seeds, limit 4), genuine 4,671 / 39 / 0; `invent --seeds 4` 64/64, 32/36, 0/24, WRONG 0; scale and noise runs WRONG 0.00; compare at 20 and 200 units 100 % / 100 %, 0 traps. 142 tests (3 new).

**The floor.** The naive Python memorizer (`tools/exam_adapt_baseline.py`) on the seed-1 streams: 20.1 % / 17.5 % facts right at 10 % / 20 %, and **32 / 59 wrong facts that repeat a believed mistake** (baby: 14 / 39), elsewhere 0 (it learns no law, so it cannot be damaged that way).

**Honest limits.** The counts that moved were 0 / 1 / 1 and 0 / 1 / 0: small, and found on three seeds only; the fix removes what was seen and the mechanism is understood, but this is again a spot check, not a proof (every harder test found a new hole). The wrong teacher is mine: persistent per fact, wrong by 1-9 on the true answer, independent of difficulty; the streams are the invention battery's tidy kinds, one rate per run. The baby still believes its teacher on a repeat (14-39 repeats): the tier measures that, it does not fix it, and a source consistently wrong about a whole kind still cannot be caught from inside.

## The entry exam: a way to judge any system, not only this one (2026-10-22; Vision Entry 10)

Entry 10 says every new version is rebuilt from scratch with the previous work as reference only. What can carry over is therefore not code but **the exam**, so I made it independent of the baby's internals (`exam.rs`, `baby exam`).

- `baby exam write DIR [--units N] [--seed N]` writes plain tab-separated files and a `README.txt` of the protocol: **`lessons.tsv`** (`id, question, answer, unit, fact`: the unit names the kind, `fact` numbers lessons that say the same thing in different words; both optional to use), **`calibration.tsv`** (optional extra material: other phrasings of taught facts, **disjoint from every question**), **`questions.tsv`** (`id, group, question, truth`; groups `taught`, `reworded`, `never`, `beyond`, `trap`; a trap's truth is empty: it must be refused). A system, in any language, writes **`predictions.tsv`** (`id, status, answer, confidence`; status `fact`, `guess` with its own probability, or `refuse`).
- `baby exam score DIR predictions.tsv` judges by rules that do not depend on how the system works: **WRONG facts must be 0**; **traps answered (as fact or guess) must be 0**; then facts right per group (coverage), guesses right, and whether the confidence it states for its guesses is at or below how often they were right. Every offender is listed by id. A system that refuses everything is honest and covers nothing: coverage is what is then compared.
- `baby exam run DIR [--out FILE]` teaches the baby and judges it: the bar a successor starts from. `tools/exam_baseline.py` is a deliberately naive foreign system (a Python exact-match memorizer) to show the judge works across languages and to give the floor.

**Reference results** (seed 1, the scale generator's worlds):

| exam | system | taught | reworded | never | beyond | traps answered | wrong facts |
|---|---|---|---|---|---|---|---|
| 60 units (2,520 lessons, 4,832 questions) | baby | 100 % | 100 % | 94.9 % fact + 5.1 % right guess | 100 % right guess | 0 of 140 | 0 |
| 60 units | naive memorizer (python) | 100 % | 0 | 0 | 0 | 0 of 140 | 0 |
| 200 units (8,580 lessons, 16,997 questions) | baby | 100 % | 100 % | 91.0 % fact + 9.0 % right guess | 100 % right guess | 0 of 467 | 0 |
| 200 units | naive memorizer (python) | 100 % | 0 | 0 | 0 | 0 of 467 | 0 |

(Percentages are facts right unless said; the baby's whole 60-unit run, from the lessons to a judged scorecard, takes 0.5 s; its guesses state 50 % (no checks yet) against 100 % realized, which the judge calls honest. Tidy rule-shaped worlds are its home ground: this is the *floor* a successor must clear and then exceed on harder material, not an achievement.)

**What the exam found in the baby on its first run (recorded as it happened).** My first version answered **two traps as fact** (22 traps at 8 units: "what is the gedevubi of ridoba **sedavubi**", two attributes at once) and refused 20 % of reworded questions. Two causes, both in my exam, both informative about the baby: (1) it was given no calibration material, and the baby's recognition floor, which is what keeps it from answering a question that merely resembles a taught one, is calibrated on rephrasings; (2) I numbered every lesson as its own fact, discarding the teacher's grouping (which lessons are one fact in different words), which the baby's cross-validation and memory rely on. With the grouping (`fact` column) and a disjoint half of the rephrasings as calibration: 0 of 22 traps, 100 % of reworded. **So the baby's trap-honesty depends on its teacher's grouping and on calibration material; a successor should state what it needs, and the exam now offers both openly.**

**Limits.** The exam is the scale generator's tidy, number-word, rule-shaped worlds: no real text (SVAMP and ASDiv cannot be redistributed: their licenses, and `bridge` already runs the baby on them locally); static (the interactive parts, the baby asking questions of a teacher, a stream of corrections, are not in it); coverage and honesty only, no joules (the energy meter is the baby's and the machine's); a successor that learned nothing but memorized the answers to `taught` would pass the honesty rules and win only that group.

### The adaptive tier (same day)

The static exam cannot test the thing the project is about: learning from being told. The adaptive tier does. `baby exam adapt-write DIR [--seed N] [--per-kind N]` writes `lessons.tsv` and `stream.tsv` (787 questions at the defaults: for each of the invention battery's 45 kinds up to 18 questions, a third unseen-inside-the-range and the rest beyond it, shuffled; each tagged with a role: **base**, **hidden**, **stage2** (kinds that can be worked out), **deceptive** (kinds that look exactly like a simple rule on the numbers taught and change regime outside them: a sum that saturates at 30, one that wraps at 40), **unexplainable** (no rule: must stay unexplained)). A static file cannot enforce "answer before you are told", so the exam **drives the system**: `baby exam adapt-drive DIR COMMAND...` starts it as a child process and speaks a line protocol on its standard input/output, one tab-separated message per line (`TEACH id question answer unit fact` per lesson, `READY`, then for each question `ASK id question` -> `ANSWER id status answer confidence`, then `TOLD id truth unit`, and `QUIT`), so the system can be written in any language and cannot see an answer before it has given its own. `baby exam adapt-run DIR` runs the baby on the same stream in-process. The judge reports facts right, wrong facts (must be 0 over the whole stream), guesses right and refusals per role, and **how many answers are right in each third of the stream**: a system that learns from being told climbs, one that only looks things up stays flat.

**Reference results** (seed 1; 3,048 lessons, 787 questions):

| role | n | baby: right in the 1st / 2nd / 3rd third | naive Python memorizer |
|---|---|---|---|
| base | 158 | 100 -> 100 -> 100 % | 0 -> 0 -> 0 % |
| hidden | 288 | 100 -> 100 -> 100 % | 0 |
| stage2 | 155 | 98.1 -> 100 -> 100 % | 0 |
| **deceptive** (changes regime) | 90 | **90.6 -> 93.8 -> 100 %** | 0 |
| unexplainable | 96 | 3.2 -> 12.5 -> 9.1 % (91.7 % refused) | 0 |
| all | 787 | 87.1 -> 88.5 -> 88.5 % | 0 |

**0 wrong facts for both systems**: the baby states 64.8 % of all questions as facts, all right, and gives another 23.3 % as right guesses and 0.5 % as wrong guesses. (The baby's run takes about 7 minutes: every told answer makes it think its unit through again, which is its cost of adapting, and one of the things a successor should beat.)

**Limits.** The tier is the invention battery: tidy, number-word, rule-shaped kinds with a simulated teacher who is always right and answers every question; no wrong teacher, no real text, no energy; the unexplainable kinds show 8.3 % right facts (8 of 96). Traced with `adapt-run --out` (which saves the replies): every one of the eight is a number the stream had already asked and been told, asked again in other words ("kakosido 5" after "what is kakosido 5" was told 1; "2 mekosido" after "what is 2 mekosido" was told 7...), so it is the baby remembering a told fact in any phrasing, not explaining the kind: new numbers in those kinds stay refused. (I first wrote this down as a repeat, then "corrected" it from a miscount of the duplicates in the file: 88 distinct of 96 questions is eight repeats, not four; the trace settled it.)

### Joules in the exam: capability per joule (2026-10-23)

Entry 7 says the measure that matters is **capability per joule as the machine measures it**, so the exam now measures it. `baby exam run`, `adapt-run` and `adapt-drive` report, after the verdict, the energy of **teaching** and of **answering** separately, read from the CPU package's own counter (RAPL) as joules above the machine's idle draw, with the guards the energy meter has accumulated (idle baseline re-measured before and after, an invalid reading shown as n/a, a warning if other processes used the CPU); very short runs are repeated and averaged so that the window is long enough to mean something (`repeats` in the report). A system driven over the pipe protocol is measured the same way: its process is on the same counter, and it says where teaching ends by replying `READY` (the protocol gained that acknowledgment). Measurement is on by default where the counter is readable (`--no-joules` for the static run, `EXAM_NO_JOULES=1` for the adaptive ones, whose command comes last on the line so cannot take a flag). **Quiet machine required** (the handover's rule): these were taken with the browser closed and no other job running.

| run | teaching | answering | per lesson / per question | right answers per joule |
|---|---|---|---|---|
| baby, static, 60 units (2,520 lessons, 4,832 questions) | 679-688 mJ (0.32 s) | 279-285 mJ (0.13 s) | 273 uJ / 59 uJ | 4,822-4,897 overall; 16,400 once taught |
| baby, static, 200 units (8,580 lessons, 16,997 questions) | 2.48 J (1.15 s) | 1.17 J (0.51 s) | 289 uJ / 69 uJ | 4,533 overall; 14,200 once taught |
| **baby, adaptive stream (3,048 lessons, 787 questions, learning from every told answer)** | 34.6 J (18.8 s) | **1,419 J (801 s)** | 11 mJ / **1.80 J** | **0.5** (693 right answers for 1,453 J) |
| naive Python memorizer, adaptive stream | 16 mJ | 61 mJ | 5 uJ / 78 uJ | 0 (it answers nothing right) |

**What this says, plainly.** Lookup after learning is nearly free for the baby: about 60 microjoules a question. **Adapting is not**: each answer it is told makes it think its unit through again, 1.80 J per question, **about 30,000 times the cost of answering without learning and about 41 times what teaching all 3,048 lessons cost**. The adaptive stream's 87-88 % right answers and the climb on the rule-changing kinds (90.6 -> 93.8 -> 100 %) are bought at 0.5 right answers per joule, against 16,000 for pure answering. That is the baby's weakest number against its own stated measure and the first thing a successor (or the next stretch of baby) should beat: learn from a told answer without re-thinking the unit. (The Python system's 0 is not a result about energy efficiency: it is cheap because it does nothing; the right-answers-per-joule figure only means something between systems that get comparable answers right.)

**Beating that number (same day).** Reading the code showed the waste: for each told answer `learn_wondered` re-decided the unit's way of learning (a cross-validation of every mechanism) once to record a confirmation, and twice for a surprising answer (`learn` had already refitted it). A settled rule, selection or exact program (reliable on facts held back, no competing explanation) now just takes the lesson in and extends its reach (`settled_exact`), and re-decides from scratch every 25 lessons (`SETTLED_RETHINK_EVERY`). **Measured on the same stream, twice on an idle desktop: answering and adapting 1,419 J -> 397 J and 403 J (801 s -> 235 s), 1.80 J -> 0.51 J per question, 0.5 -> 1.6 right answers per joule (692 right answers for 445 J)**; teaching is unchanged (34-43 J: the 20 % spread between runs is the meter's other-process share, which it flags as an upper bound). The answers did not change in any way that matters: 0 wrong facts; the rule-changing kinds still climb 90.6 -> 93.8 -> 100 %; 692 right answers against 693; one kind (stage2) is two refusals worse in the first third of the stream. The honesty gate: hidden 64/64, unexplainable 0/24, **careful battery deceptive 718 / 2 / 0 (was 704 / 16 / 0)**, genuine 4,671 / 39 / 0, WRONG columns 0.00, 20 %-persistent teacher 87.40 % / 39 wrong all repeats / 0 elsewhere (was 87.31 % / 42), slips 99.97 % / 0, corrections 157/157, 0 of 467 traps, test suite 137 passing in 18 s (was 25 s). Still 8,400 times the cost of answering without learning (0.51 J vs 59 uJ); where the rest goes (the surprising answers' refits, `repair_regime`, the periodic re-decisions, the guess scales) has not been measured.

**Where the rest went, and a second cut (same day).** Stage timers (`BABY_PROF=1`; `school::timed`, printed after `adapt-run`) over the 787 told answers (about 235 s of CPU time at that point) gave: `relearn_unit` called from `learn_wondered` **145 s (62 %)**, 140 calls at ~1 s each; `review_programs` 45 s (19 %); `learn` itself 50 s (`refit_in_place` 32 s over 272 surprising answers at 118 ms, 12 full re-decisions 18 s). Most of the 140 re-decisions were units that can only memorize (the unexplainable kinds), which `refit_in_place` already re-tests only every ten facts, but `learn_wondered` re-decided on every told answer. They now follow the same every-ten-facts rule (`memorize_only_known`): re-decisions 140 -> 56, 145 s -> 50 s, listed total 290 s -> 192 s, **answers unchanged** (the ALL row 86.7 -> 88.5 -> 88.5 % identical, 0 wrong facts). **Measured on an idle desktop: answering and adapting 268 J (150 s), 0.34 J per told answer, 2.2 right answers per joule overall (692 right for 313 J), against 1,419 J / 1.80 J / 0.5 at the start of the day: 5.3 times cheaper.** The meter's usual warning (other processes up to 25 % of the CPU) applies, so read 268 J as an upper bound. Not cut, deliberately: `review_programs` is free almost always (788 checks cost 0.02 s) but runs four times in the stream, about 11 s each, because each time the shared library gains a building block every open unit is examined again; that is real work for a real change. The honesty gate after this change: identical to the one before it (careful 718 / 2 / 0, 0 wrong anywhere, 0 of 467 traps, corrections 157/157, 20 %-persistent 87.40 % / 39 / 0 elsewhere).

**Limits.** The counter reads the whole CPU package above idle, to within about +-30 % on a desktop; the baby's static timings repeat to 2 %; the adaptive figures are one run each. They do not include memory, disk, a GPU or the machine's idle draw. The Python system's interpreter start-up is inside its teaching figure.

## A person answers the baby's questions: `baby tutor` (2026-10-22)

Until now the questions the baby asks about its own explanations (`baby wonder --check`: the farthest number it has not checked, the corner, the swapped order) were answered only by simulated teachers; the CLI could list them and take one answer at a time with `baby learn --wondered`. `baby tutor [--school FILE] [--rounds N] [--limit N] [--out FILE]` runs the whole loop with a person (`tutor.rs`, `tutor::session`): it shows each question, **why it asks**, and **what it would say now** (a fact, or "probably X, a guess, 50 % sure"), then you type the answer, press Enter to skip, `?` if you do not know, or `q` to stop. It learns from each answer and saves the school at the end. Three behaviors:
- **Skepticism with a person.** An answer that contradicts a rule the baby has good evidence for is not believed on one word: it says what it would have said and asks you to type the answer again. The same answer twice is believed; a different second answer leaves it unsettled with the rule kept; Enter keeps the rule. (A slip of the finger costs one more question; a genuine contradiction, such as a sum that really caps at 30, is believed when you repeat it.)
- **Your own lessons through the same door.** At any prompt `a question => the answer` is placed by `School::learn_told`: filed under the unit whose program turns the question's numbers into that answer (cut down to those numbers), or refused with the reason if no program can place it.
- **Digits become the unit's words.** If the unit speaks in number words and you type `6`, it is taken as `six` (up to ten).

The person is a function (`FnMut(&Prompt) -> Said`), so the terminal and the tests run the same code. Tests (131 in all): a person who answers what is asked teaches the cap at 30 and the corner it covers (a fact afterwards, not a guess); a wrong answer followed by a different one is never believed and a genuinely contradicting repeated one is; a skip keeps the rule; a free-form line is placed. Tried for real by piping answers into the command on a freshly taught school (22 questions in the first round).

**Honest limits.** Only simulated and scripted people have answered it; the questions it asks are the ones `check_myself` knows how to ask (numeric units; nothing yet about facts or phrasings, or "which number matters"); it does not yet pick the question whose answer would tell it the most; a real person repeats themselves more than a simulation does.

## Being told the truth: learning from a stream of corrections (2026-10-22; Vision Entry 9: getting better so it makes fewer mistakes)

**The idea.** When the baby is told the true answer to a question it guessed at, the lesson should go where it belongs and fix what made it guess wrongly. The old `learn` files it in the unit the question already resembles, which is the unit that was wrong. `School::place_told` does what the selector's labels already did: it looks for the **one unit and one choice of numbers whose program turns those numbers into the told answer**, and files the question there, cut down to those numbers; `School::learn_told` takes the lesson in only then. Every 20 placed lessons it learns the selector (from the examples it was taught plus the pairs it has now been told) and the routing corrections again. A guess-only unit is not re-fitted for every lesson (its tolerant program search costs seconds); it takes the lesson into what it remembers and how questions are routed at once and looks at its program again every 25 lessons (`GUESSER_REFIT_EVERY`, only on this path).

**Measured as a stream** (`baby bridge ... --online`, flag last): after the usual two-step teaching, the held-out questions arrive in a shuffled order; for each one the baby is asked, the guess is scored, and then it is told the answer and learns. The static column asks the same questions in the same order with nothing learned.

**A negative result first, and its cause.** My first version filed *every* told answer, wherever the question routed: nothing improved (guesses right 47.5 -> 47.2 %, 43.7 -> 38.1 %, 46.3 -> 39.2 % over the stream on SVAMP blocks) and it got **worse late in the stream** (37.5 -> 15 % in the last windows). Cause: only 49-58 % of SVAMP's answers are reproduced by any program (the rest are the questions it gets wrong or cannot cut down), and filing those as lessons into a unit whose program cannot explain them drags its explained share under 80 %, so the unit loses its program and falls back to memory. A lesson a unit's program cannot explain is not a lesson for that unit. Hence "place it or do not take it in".

**With placement** (5 streams, 0 answers stated as fact wrong in any of them):

| stream | guesses right: static -> learning | routed to own kind: static -> learning | answers it could place | placed into the file's real kind |
|---|---|---|---|---|
| SVAMP blocks (1) | 47.5 -> 47.8 | 80.6 -> 80.2 | 68.8 % | 97.3 % |
| SVAMP blocks (2) | 43.7 -> 45.2 | 74.9 -> 75.5 | 79.3 % | 93.4 % |
| SVAMP blocks (3) | 46.3 -> 47.5 | 77.5 -> 76.9 | 72.8 % | 94.9 % |
| SVAMP random (1) | 41.9 -> 42.6 | 76.7 -> **80.7** | 68.6 % | 93.6 % |
| ASDiv random (1) | 56.7 -> **59.2** | 76.5 -> 78.6 | 90.3 % | 99.1 % |

The collapse is gone and the direction is right in all five, but the gain is **small** (about +0.3 to +2.5 points of guesses right over a stream of 240-324 questions). The placement inference is accurate (93-99 % name the real kind). Why small: the errors that remain are mostly wording (addition vs subtraction told by "how many more" vs "in all"), not a shortage of lessons, and the stream is only a few hundred questions. The window-by-window curves are noisy (+-5 points) and no single window should be read.

**Honest limits.** Streams of 240-324 questions, one run each for SVAMP random and ASDiv, three for blocks; the gain is within the window noise for individual windows; placement needs a program (about 49-58 % of SVAMP's told answers cannot be placed and are simply not learned, which means the questions it cannot yet answer are the ones it learns least from); the teacher is the file (always right). Tests: `an_answer_it_is_told_is_filed_where_a_program_reproduces_it_and_one_it_cannot_place_is_not_taken_in` (128 tests).

## A second corpus: ASDiv, and what a re-count did to SVAMP's figures (2026-10-22)

**The corpus.** ASDiv (Academia Sinica Diverse MWP dataset, 2,305 problems; `dataset/ASDiv.xml`, 947,401 bytes; **CC BY-NC 4.0**, so used for private evaluation only: downloaded with Daws's explicit yes to scratch space and **not committed**). The 960 problems of the four kinds SVAMP shares (Addition 281, Subtraction 331, Multiplication 194, Common-Division 154; its other 20 kinds (geometry, ratio, gcd...) are set aside unless `--kinds all`). Unlike SVAMP it writes many numbers as **words** ("Seven red apples and two green apples..."), which the baby reads. `baby bridge --asdiv ASDiv.xml ...` (`bridge::load_asdiv`, a minimal XML reader; the answer is the number before the unit "9 (apples)").

**First contact, as with SVAMP:** taught on all 885 readable problems, it learns nothing and states nothing wrong (0 right, 0 wrong; every unit "remember, 0 %"; routing alone already sends 65 % to the right kind).

**Two-number problems, nothing retuned for this corpus** (3 seeds, random split, 678 problems): held-back program scores 68-92 % per kind; routing to the right kind **76.3 / 76.6 / 76.7 %**; labeled guesses **57.6 / 59.6 / 58.2 % right** and 35.6 / 29.8 / 34.9 % wrong; the stated hit rate is at or below the realized one in all three (58.3 vs 61.8, 55.3 vs 66.7, 56.6 vs 62.5); told the kind (oracle) 74-78 % right; nothing stated as fact. This is the same picture as SVAMP's, from a differently built corpus, with the routing corrections and last-words features unchanged.

**With number selection** (`--select`, 3 seeds): the arithmetic part of ASDiv has few distractors (only 63-70 of about 600 readable lessons hold more than two numbers, 54-58 of them labeled by a program), so this is a small-number test: problems with two numbers or fewer 56.5 / 58.9 / 58.9 % right as labeled guesses; problems with more than two numbers (only 24-31 test questions per seed) 58.1 / 45.8 / 54.2 % right, stated 44.8 / 54.3 / 51.3 % after the baby scaled its claims (x0.78-0.93) from held-back answers; the selector kept the right pair in 72.0 / 59.1 / 76.2 % of the 21-25 problems where that is checkable. Too few to rank against SVAMP's 27-38 %; it says the mechanism transfers and the calibration holds.

**Two mistakes found on the way.**
1. *My first ASDiv selection run gave guesses to only 13 % of questions* (the plain run: 93 %). Cause: 114 of 960 problems carry zero or one number the baby can read ("a dozen", "twice", "half"); a program combining two numbers has nothing to work with in those lessons, and they pushed units past the tolerance for unreadable lessons. They are now set aside as unreadable ("fewer than two numbers it can read"), like the other unreadable cases.
2. *That changed SVAMP's figures, and I should say so.* `numbers_in_text` counted runs of digits only; the baby reads number words too, so a SVAMP problem with two digit-numbers and a story word "one" was being taught as a "two-number" problem though the baby saw three. It now counts numbers as the baby reads them, and the filter above applies to SVAMP too. **Re-measured on all six SVAMP two-number splits, the figures are lower and more variable than the ones I reported earlier today** (routing 64-75 %, mean 70 %, was 74-78 %; labeled guesses right 35-73 %, mean 56 %, was 47-74 %, mean 63 %): 

| split (seed) | routing to own kind | guess right | guess wrong | units that fell back to memory |
|---|---|---|---|---|
| random (1) | 69.8 | 61.3 | 35.8 | multiplication |
| random (2) | 73.7 | 72.6 | 26.3 | multiplication |
| random (3) | 71.7 | **35.4** | 22.2 | multiplication, common-division, addition |
| blocks (1) | 67.6 | 56.8 | 27.9 | addition, multiplication |
| blocks (2) | 75.0 | 69.4 | 25.9 | multiplication |
| blocks (3) | 64.2 | **40.8** | 29.2 | multiplication, common-division |

The swings come from whether a kind gets a partial program at all. In random 3 the common-division program scored 55 % instead of 89 % on a slightly different set of 240 lessons and that kind went from 90 % right to no guesses at all. **My first explanation was the 0.6 guess bar; that was wrong.** Running the six splits with the bar at 0.6, 0.45 and 0.3 gave identical output to the decimal. A debug print of the partial-program fit showed it only ever reaches its acceptance test for the subtraction unit (`first apart second`, 84-91 % of lessons explained, ample bits); for the units that fall back to memory in those splits the search finds no expression that explains 80 % of the lessons within its budget, so the unit is rejected earlier, before any bar applies. **Three hypotheses tested and rejected on the fragile split (random, seed 3), each giving output identical to the decimal:** the guess bar (0.6 / 0.45 / 0.3), the search's work budget (1 M / 4 M / 16 M candidates), and the ambiguity cap inside the partial search (a print there fires only for the subtraction unit, with one smallest expression each time). For the units that fall back to memory the fit never reaches the partial-program search (or its acceptance test) at all; where it leaves the path before that is not found. **Found, by printing inside `fit_trusted_with` and `choose_for_unit`.** The failing kinds did get a program: `first x second`, `(larger div smaller)`, `first + second`, found as *exact* programs (the plurality of each input's lessons settles the points, so a handful of wrong lessons do not stop the search). But their held-back scores (80 %, 55 %, 77 %) are under the reliability bar, so the unit was judged "memorize only", and the fallback that keeps a program as a guess-only one demanded the program be flagged *partial*, which an exact fit never is. A program right on the plurality of its inputs but not reliable enough to state facts is exactly what a labeled guess is for. **The fix:** such a program (held-back score between the guess bar and the reliability bar) is *provisional*: `ProgramRule::make_provisional` flags it with the same `partial` mark as a program that fits most lessons, so it can only ever give labeled guesses; `choose_for_unit` accepts it; `fit_for` applies it where a unit is fitted (`learn_items`, `relearn_unit`). **Result** (SVAMP two-number, six splits, guesses right before -> after): random 61.3 -> 61.3, 72.6 -> 72.6, **35.4 -> 41.4**; blocks **56.8 -> 61.3**, 69.4 -> 69.4, 40.8 -> 40.8: mean 56.1 -> 57.8 %; wrong labeled guesses rise a little where it now guesses more (e.g. 22.2 -> 31.3 % in random 3); stated vs realized stays within 4 points where it changed (60.9 vs 56.9, 63.9 vs 61.3); ASDiv (already all programs) unchanged to the decimal. The honesty gate is unchanged on every line (hidden 64/64, unexplainable kinds that got a mechanism 0/24, careful 704 / 16 / 0, WRONG columns 0.00, 0 of 467 traps). What is left on the fragile split is classification, not learning: the addition kind is a program again but 79 % of its questions get a wrong guess (they are sent to the larger subtraction unit), and division (55 %) is still under the guess bar. Test: `a_program_right_on_most_inputs_but_not_reliable_enough_to_state_facts_may_only_guess` (127 tests). The SVAMP tables earlier on this page were measured under the old count and stand as history.

**Cost, found and fixed.** Building a school on ASDiv's 678 problems took 222 s: 111 s cross-validating the programs, and then **another 111 s** in `review_programs` repeating that for every unit because the library "might have changed". When the library never gained anything (these partial programs are never vocabulary) nothing can have changed for any unit, so the review is now skipped in exactly that case: **identical output, 222 s -> 111 s**; the tidy-world gate reads exactly as before. **Retracted and fixed (later the same day):** I wrote here that the remaining 111 s was the program search's cross-validation. Measured, that was wrong: the program search takes about **1 s**; memory and select are free; the **rule** mechanism's ridge fit took about 11 s per fit, and with up to 13 robust rounds per fit and a cross-validation around it that was over 90 % of the build. The cause: the ridge fit solved a dense system of 3 x (number of words) unknowns by Gaussian elimination, and on real text the vocabulary is thousands of story words, which is cubic. With more unknowns than lessons the same fit has an equivalent *dual* form whose system has the size of the number of lessons (the Woodbury identity: theta = L^-1 X^T (I + X L^-1 X^T)^-1 y, to rounding), so it is now solved that way when 3 x words is at least 600 and more than twice the lessons (`Quantity::fit_ridge_dual`; every tidy-world fit stays on the original path). **ASDiv build 111 s -> 13.7 s, SVAMP two-number runs 13 s -> 6 s, outputs identical** (compared line by line on ASDiv and four SVAMP splits), and the tidy-world gate reads exactly as before (hidden 64/64, unexplainable 0/24, careful 704 / 16 / 0, WRONG columns 0.00, 0 of 467 traps, 20 %-persistent 87.31 % / 42 / 0 elsewhere). Test: `the_dual_form_of_the_ridge_fit_gives_the_same_rule_as_the_primal_form` (132 tests). A first attempt that removed per-candidate allocations from the program search changed nothing (12.8 s vs 12.4 s): I had optimized the part that cost 1 s. `perf` is not available here (kernel setting; not mine to change), so the profile came from timing each mechanism.

**Honest limits.** ASDiv's arithmetic subset is mostly clean two-number problems, so it hardly tests number selection; SVAMP was built by varying ASDiv's problems, so the two are related builds, not independent sources; the license limits it to private evaluation; the answers are whole numbers up to four digits.

## Real text, the problems with more numbers than the rules use: which numbers matter (2026-10-22)

**The gap.** 491 of SVAMP's 971 readable problems hold three numbers and 155 hold four; the programs the baby builds take one or two. Until now those lessons were unreadable to it, so a school taught on the whole file learned nothing (the tolerant program search refuses when more than a fifth of the lessons hold more than two numbers) and every such question was refused.

**Measured first (offline, before building).** A perceptron on the words around each number to pick the pair that the equation uses reaches only 39-40 % (chance 28 %, my earlier prototype 36-38 %). Adding one general cue, **whether the words next to a number also occur in the question**, takes it to **64 %** with the question as a separate field and **57-59 %** when the question is recovered from the single string (the last sentence: after the last sentence boundary before the final question mark); a fixed window of the last 6-12 words as a stand-in for the question only gets 49-54 %, because it mixes the last body sentence with the question.

**What was built** (`selector.rs`, `School::set_selector`, `School::focus`, `bridge::run_select`, `baby bridge ... --select`; the flag must come last on the command line). A `Selector` scores each number in a text on the words just before and after it, where it stands, and the question-overlap cues, learned by an averaged perceptron (fixed seed, 12 epochs, no dependencies); `focus` keeps the two best and blanks the others with a word (`numx`), so the rest of the baby sees an ordinary two-number question. Where do the labels come from? **From the baby's own programs, not from the corpus's equations:** first it learns its units from the lessons with at most two numbers; then for each many-number lesson it looks for the pair of numbers that its unit's program turns into the given answer and, where exactly one pair does, that pair is a label ("connecting the dots"). It learns the selector from those, teaches again with the unused numbers of those lessons blanked, and asks every question through the selector. The selector and the rate scales below are saved in the school file (`selector N` / `sw` lines, `guessscale`), and a question with two numbers or fewer is never touched.

**Honest stated rates needed their own fix.** The first run was badly over-confident: the many-number questions stated 77 % and were right 32 %, and even the two-number ones stated 81 % and were right 62 %. Two causes: a guess from a question cut down to two numbers is right only if the cut was right (the stated rate ignored that), and the labeled lessons are exactly the ones a program already explains, which inflates each unit's held-back score. Fix: `School::calibrate_guesses`. The harness holds back every fourth lesson, teaches on the rest, lets the baby ask itself those questions, and rescales its stated hit rates to what it really achieved, separately for questions read as written and for ones cut down (never up); then it is taught on everything and carries the scales.

**Numbers** (SVAMP, 3 seeds x random/blocks; tested on all readable problems held out, about 290-325 per split; seeds 2 and 3 rerun after I found that `--seed` had been swallowed as the value of `--select`, so the first three runs were all seed 1). Nothing is stated as a fact in any of them (0 right, 0 wrong as fact):

| split (seed) | two numbers or fewer: guess right / wrong | more than two: guess right / wrong | stated vs realized, two or fewer | stated vs realized, more than two | selector kept the used pair | routed to own kind |
|---|---|---|---|---|---|---|
| random (1) | 61.5 / 38.5 | **32.2** / 67.3 | 65.6 vs 61.5 | 23.7 vs 32.4 | 56.8 % | 74.1 % |
| random (2) | 61.7 / 33.0 | **35.2** / 64.2 | 55.5 vs 65.2 | 15.9 vs 35.4 | 55.9 % | 79.8 % |
| random (3) | 62.4 / 36.6 | **37.8** / 60.0 | 59.3 vs 63.0 | 19.3 vs 38.7 | 57.8 % | 78.4 % |
| blocks (1) | 69.0 / 28.3 | **33.6** / 64.9 | 78.3 vs 70.9 | 31.1 vs 34.1 | 58.9 % | 76.3 % |
| blocks (2) | 55.9 / 32.4 | **27.4** / 71.7 | 72.9 vs 63.3 | 29.7 vs 27.6 | 46.0 % | 62.7 % |
| blocks (3) | 47.4 / 19.0 | **28.8** / 59.6 | 52.5 vs 71.4 | 26.2 vs 32.6 | 54.5 % | 76.4 % |

Before: **0 %** right on the many-number questions (nothing learned, everything refused). Now 27-38 % of them get a right labeled guess. That is not good, and the table says why: about 60-72 % of them get a wrong labeled guess (the selector keeps the right pair about 46-59 % of the time, the kind is recognized 63-80 %, the program is right 67-91 % once told the kind: the product is about a third). What changed is that the baby now says so: for these questions it states 16-31 %, below what it achieves in five of six splits (the exception, blocks 2, is 2 points over), while for the two-number questions its stated rate is within 10 points in five of six splits (over by 4-10 points in three of them: 65.6 vs 61.5, 78.3 vs 70.9, 72.9 vs 63.3; 19 points under in blocks 3: 52.5 vs 71.4). The two-number guesses are a little worse than in the earlier two-number-only runs (about 47-69 % against 57-74 %): the units now also hold the focused many-number lessons, and these test sets are different.

**Mistakes (recorded as they happened).**
- My first claim of "which numbers matter" was a perceptron on local context alone: 39 %, no better than before. The cue that mattered was the question overlap, found by asking what makes a distractor a distractor.
- A fixed-length tail window as the question proxy lost most of the gain (49-54 % vs 57-59 %): the question has to be found as a sentence.
- `--seed` before `--select` was parsed as the value of `--select`: three of my "seeds" were seed 1 and their identical outputs were the clue. The option parser takes the next token as a value; flags go last.
- Stated rates were wildly over-confident until the self-check was added (77 % stated, 32 % right).

**Honest limits.** One adversarial corpus. The selector is learned from labels that exist only where exactly one pair fits (about 61 % of the many-number lessons in the first run): problems whose answer needs three numbers (237 of SVAMP's equations use three) are not handled at all. The stopword list, the 8-feature window sizes, the kept count (two), the one-in-four held-back share, the 10-guess minimum for rescaling and the epochs are mine and were not tuned. The scales are measured on 25 % of the lessons only. It does not yet choose its own questions about which number matters.

## Varying the gate again: damage under a persistently wrong teacher that seed 1 hid (2026-10-23)

**The sweep.** Having been caught once by a gate that ran at one setting, I varied the others: the careful battery on seeds 4-9 at 1 and 2 questions per unit (`invent --careful --first N`, new), the scale test on seeds 2-4 at label noise 0 / 2 / 5 / 10 % (and with digits), the invention battery on seeds 9-16 and at noise 2 / 5 / 10 %, the wrong-teacher experiments on seeds 2 and 3 (20 and 200 units, slips and persistent), and the exam on seeds 2 and 3. **The careful battery (12 runs), the invention battery (permissive, 8 seeds; noisy, 3 settings) and both exams were clean** (0 wrong facts, 0 traps). Two things were not:
- **Label noise, other seeds: a pre-existing limitation, not a regression.** Reworded questions show 0.2-0.8 % WRONG at 2-10 % noise on seeds 2-4 (the seed-1 gate showed 0.00). The pre-today build gives byte-for-byte the same numbers (compared on all eleven runs), and they are the baby repeating, in another phrasing, a fact it was wrongly told: the documented invariant, graded against the truth. Recorded, not chased.
- **A persistently wrong teacher, seed 2 and 3: a regression from my repair step, and a real one.** On seed 2 at 20 units the wrong facts went from 1 (build before the repair step) to **71, 68 of them damage elsewhere**; at 200 units from 40 (0 elsewhere) to **158, 100 elsewhere**; seed 3 went 31 -> 45 and answered **one trap** (0 of 467 before). The seed-1 gate had read "0 elsewhere" because seed 1 happens to be benign.

**Cause** (bisect: present from the first repair commit; `WRONG_DEBUG` and a path trace). A bigger-of selection unit ("which is X, eight or zero") was taught only 0-5. The teacher's persistent wrong answers for "ten or five" and "ten or nine" lay beyond that; the repair rebuilt a law around them, `second max (first min 7)`, using constants taken from the wrong answers. My guard checks the repaired law against the old one only where the *other* lessons vouch (0-5 x 0-5, where the two agree exactly), so it passed. Later correct lessons that happen to agree with both laws (ties, or the other number larger) then counted as confirmations and vouched for everything below them, including (8,0), where the old law was right and the new one said 7. 68 questions were answered wrongly as facts.

**The fix is a principle the project already used elsewhere** ("answer only where competing explanations agree"): the law a repair replaced stays a competing explanation. A repaired program remembers what kind of law it replaced (`before`, saved with the school; the old law itself is re-derived on demand from the unit's other lessons, cached, not saved). **Until `RETIRE_AFTER` = 2 lessons told after the repair have separated the two (the old law wrong on them, the new one right), a fact the new law would state that the old law contradicts is demoted to a labeled guess** (to *no answer* on a refuted unit, and where the old law is **silent**: silence is not endorsement, which is how the one answered trap, "two bikivubi five", a negative result that a coincidence law turned into "zero", got through my first version). A question it was told exactly stays a fact. `School::repair_veto`; tests: `a_rebuilt_law_does_not_overrule_the_one_it_replaced_until_lessons_told_after_the_repair_have_separated_them` (139 tests).

**Result** (9 teacher jobs, seeds 1-3, 20 and 200 units, slips and persistent): **0 damage elsewhere in every one, 0 traps answered**; seed 2 at 20 units 71 -> 3 wrong (all repeats), right 88.5 %; 200 units: 20 %-persistent right 87.4 / 74.9 / 82.0 % on seeds 1 / 2 / 3, wrong facts 39 / 52 / 45, all repeats of facts it was wrongly told. The rest of the gate is unchanged (careful battery 0 wrong at 1, 2 and 4 questions per unit: 331 / 478 / 480 of 480 right; the standard battery 684 / 36 / 0; corrections 157/157; hidden 64/64; scale 0.00; adaptive exam 0 wrong, rule-changing kinds 90.6 -> 93.8 -> 100 %).

**Costs, stated plainly.** Under a persistently wrong teacher the baby answers less than the build before the repair step (right 74.9 vs 79.2 % on seed 2, 82.0 vs 88.8 % on seed 3, 87.4 vs 88.0 % on seed 1) and states somewhat more repeats of wrongly-told facts (52 vs 40, 45 vs 31): it is more willing to believe a rebuilt law, and what it was wrongly told is recalled in any phrasing. I judge that the right side of the trade: no wrong facts about things it was not wrongly told. The facts the rebuilt laws used to supply beyond a vouched region are now guesses until two separating lessons arrive.

**Tried to win the coverage back, and it made things worse (reverted, 2026-10-23).** Idea: a rebuilt law only retires its predecessor after two separating lessons, so have `check_myself` ask, for each such unit, the questions where the old law and the new one part ways (the nearest to the taught numbers first, then the farthest from that), so that one or two honest answers decide. Built (`separating_probes`, a test, 140 passing) and run through the same 18-job gate. **Results against the veto alone:**

| | veto alone | with separating probes |
|---|---|---|
| persistent teacher, 200 units, seeds 1 / 2 / 3: right | 87.4 / 74.9 / 82.0 % | 88.5 / 78.5 / 86.0 % |
| persistent teacher, 20 units, seed 2: right | 88.5 % | **97.7 %** |
| persistent teacher, 200 units, seed 2: wrong facts (elsewhere) | 52 (**0**) | 92 (**32**) |
| careful battery, deceptive kinds right of 480-720, budgets 4 / 1 / 2 | 684 / 331 / 478 | **678 / 288 / 442** |
| everything else (corrections, exam, scale, invention, slips, other traps) | identical | identical |

Coverage under the wrong teacher improved, but the careful battery's rule-changing kinds fell at every budget (the separating probes spend the question budget that the ordinary edge and corner checks would have used to extend where the law may speak), and **seed 2 at 200 units gained 32 wrong facts elsewhere**: a persistent teacher's wrong answer can coincide with the rebuilt law at a probed point and count as a separating lesson, retiring the old law wrongly. An honesty regression and lower deceptive coverage are not an acceptable price for better coverage under a wrong teacher, so it is reverted. **What it taught:** retirement needs lessons that cannot be a wrong teacher's coincidence, and that is not available from the lessons alone; the veto's cost is therefore best paid back some other way (an independent second source, or checks that extend coverage rather than only separate). Not tried: asking for separating lessons only when the unit is not refuted, or using a probe's answer only if it is also repeated.

**Also changed.** `select`, `online` and `no-joules` are now boolean flags (they were not in the list, so `--select --seed 2` swallowed the seed; flags no longer have to come last). `invent --careful --first N` runs seeds N..N+seeds-1. `TRAP_DEBUG=1` on `compare --teacher` lists the traps that were answered.

## A hole found by asking fewer questions: a repair swapped for a coincidence (2026-10-23)

**Found by accident, while measuring something else.** To see whether a smarter chooser of its own check questions had anything to win, I ran the careful battery at a budget of 1, 2 and 4 questions per unit per round (2 seeds, 3 rounds). The curve says the current chooser is already efficient: 2 per unit reach 478 of 480 right on the rule-changing kinds with 370 questions asked against 490 for 4 per unit. But **at 1 per unit the deceptive kinds stated 6 WRONG facts** (304 / 170 / 6 after two rounds), and they stayed. The gate had only ever run at 4 per unit.

**A regression, bisected.** The build before the repair step read 0 wrong at this budget; the four commits from the first repair commit on all read 6 (so it came with the repair itself, not with the later changes). The six were all one unit, `capsum` (a sum capped at 30), stated by `second + (first min ((3 x 3) + (3 x 3)))`. Instrumenting where its program was set showed the sequence: the first probe, (24,12) -> 30, made the repair adopt `(30 min (first + second))`, correct; the second probe, (48,12) -> 30, made `refit_in_place` fit a fresh program from scratch, **without the numbers the repair had been given**, and the smallest program that fits was a coincidence: with a library of 3 and a sum of two squares it can build 18 = 3x3 + 3x3, and `second + (first min 18)` gets both probes right (18 + 12 = 30). It replaced the repaired program and dropped its `repaired` bookkeeping, so the two probes, now ordinary confirmations, vouched for everything under (48,12): 19 + 2 stated 20 (truth 21), 27 + 0 stated 18 (truth 27), and so on. The old build could not do this because a contradicted rule just stayed refuted; my repair created a program that a lesson could then swap.

**The fix** (`refit_in_place` and `relearn_unit`): a repaired program that still explains every lesson, the one just taken in included, is not replaced by a fresh fit. Test written first (it reproduced the exact coincidence law before the fix): `a_repaired_program_is_not_swapped_for_a_coincidence_by_the_next_lesson_and_the_checks_do_not_vouch_for_a_wrong_law` (138 tests).

**Result, and its price.** 0 wrong facts at every budget:

| questions per unit per round | asked over 3 rounds | deceptive kinds: right / refused / WRONG | genuine kinds |
|---|---|---|---|
| 1 | 228 | 331 / 149 / **0** (was 325 / 149 / 6) | 2340 / 800 / 0 |
| 2 | 370 | 478 / 2 / **0** | 3108 / 32 / 0 |
| 4 | 490 | 480 / 0 / **0** | 3108 / 32 / 0 |

The rest of the gate is clean (hidden 64/64, unexplainable 0/24, 0 wrong anywhere, 0 of 467 traps, corrections 157/157, 20 %-persistent 87.40 % / 39 / 0 elsewhere), **but the standard battery's deceptive kinds read 684 / 36 / 0 instead of 718 / 2 / 0**: keeping the repaired program gives up the extra coverage a fresh fit used to find. I judge 34 facts of 720 a fair price for closing a hole that stated wrong facts, and the lost coverage should come back through better checks, not through trusting the smallest program that fits. **The gate now includes `invent --careful --seeds 2 --rounds 3 --limit 1`.**

## Real text: the routing wall was the counting, not the words (2026-10-22; Vision Entry 9/10: learn from its own mistakes)

**A wrong belief, corrected.** The previous section said routing the kind of a SVAMP problem "from the words alone" tops out at 49-62 % because "bag-of-words cannot see the compositional cue", backed by an offline prototype of three naive-Bayes-style classifiers (57-65 %). That was a fact about the **classifier family**, not about the words. Re-measuring offline with an averaged perceptron (learns from its mistakes) on the *same* bag of words: **78 %** (all 1,000 problems, five folds; grouped by story so the same story never sits in train and test: 78 %); adding the **last few words** as separate, marked features (the question of a story problem comes last): 80-82 %. The baby's routing table is a counting table (per word, per unit: ln(1 + 2 x count), minus a penalty for the words it knows), which treats every word as independent evidence, exactly the naive-Bayes weakness.

**What was built** (`Routing::refine`, `routing_features`, `settle_route_precision`). A mistake-driven correction layer over the routing: per word, one number per unit, learned by an averaged perceptron over the baby's own lessons in a fixed-seed shuffle (12 epochs), with the words of the question plus its **last 8 words marked as such** as features. It is **used only when routing is measurably poor and the corrections measurably help**: the five-fold routing of its own lessons is run both by counting and with corrections learned from the training folds only; the corrections are adopted only if they route more held-out lessons to their own unit and counting routes under 95 %. (That test only runs when some unit is a partial program, as before.) So every tidy world is unchanged by construction, and the whole gate confirms it. Adopted, the corrections are the score (learned from zero; on top of the counts they made it *worse*, 41 % vs 48 % in the folds: the counts are on a much larger scale than a +-1 nudge). Nothing is stored: `load` retrains them from the lessons, deterministically.

**Numbers** (`baby bridge --svamp ... --units kind --numbers 2`, 340 two-number problems, seeds 1-3, mean of three; random split / blocks split):

| | routing to the right kind (before -> after) | labeled guesses right | wrong as a labeled guess | stated as fact |
|---|---|---|---|---|
| random | 56.7 % -> **76.4 %** | 49.8 % -> **67.8 %** | 29.8 % -> 27.5 % | 0 wrong, 0 stated |
| blocks | 50.1 % -> **72.5 %** | 37.8 % -> **58.7 %** | 29.7 % -> 25.4 % | 0 wrong, 0 stated |

Each of the six splits improves on routing (+15 to +24 points) and on guesses right (+13 to +21). The last-words feature alone, on top of the corrections: +1 to +5 points of routing in all six splits, never worse. The stated hit rate stays at or below the realized rate in five of six splits (e.g. 66.9 vs 69.8 %, 66.2 vs 74.5 %, 70.3 vs 69.0 %, 66.3 vs 73.8 %, 61.1 vs 67.5 %) and is 8 points over in one blocks split (75.3 % stated vs 67.3 % right). Nothing is stated as fact: every answer on real text is still a labeled guess.

**Mistakes (recorded as they happened).**
- First version: corrections on top of counts, and **the samples fed in unit order**. A perceptron trained in class order forgets all but the last class: the cross-validated routing fell to 39 % (below majority) even for the pure learner. Shuffling each epoch (deterministic) took it from 39 % to 69 % in the folds. The sign of a bug was a result *below the majority class*.
- Adding the corrections on top of the counts was worse than counting (41 % vs 48 %); the pure learner is what works.

**Honest limits.** One adversarially built corpus; the units are the file's labels; 340 problems with exactly two numbers (the 3-4-number problems still learn nothing); about a quarter of all questions still get a wrong labeled guess (routing errors and the program's own); the epochs, the 8-word tail, the 1.0 margin and the 95 % trigger are mine and were not tuned (the tail length was picked from one offline run: 6, 8, 10 words gave 81.4, 81.6, 79.2 %); routing precision is still one number per unit.

## After a miss, learn the new regime: repairing a law from the lessons that contradict it (2026-10-22; Vision Entry 7: guess, get it wrong, update)

**The gap.** Until now one correction of a wrong guess only *stopped* the unit guessing beyond its reach (`refuted`): the baby learned "my rule is wrong out there", not what is true there. In `baby invent --corrections --seeds 3` (one correction per deceptive kind) all 157 other wrong guesses became refusals and 64 of the 137 right guesses were refused too. The cause was in the language, not the search: `min(first + second, 30)` is five pieces, but the base language has only the constants 1 and 2 (the library learns a ten from rules it has fitted, never a 30 it has just been told).

**What was built** (`School::repair_regime`, `ProgramRule::fit_repair_with`). When a numeric unit is refuted, the numbers the contradicting lessons point to (the answer they gave, and how far the old rule was from it: 30, or the 40 of a wrap) are offered as temporary terminals to one exact program search over **all** the unit's lessons. The program is accepted only if it clears the same believability and ambiguity bars as any program **and** agrees with the old rule at every point inside the range the uncontradicted lessons cover (see the mistake below). Then the unit *is* that program, it is no longer refuted, and the temporary numbers never enter the library. A failed search is remembered until the contradicting lessons change (finding nothing for some lessons means finding nothing for more).

**One point is not a law.** A lone correction is consistent with many regimes: "24 + 12 is 24" is explained by a cap at 24 as well as by a reflection at 30. If the corrected lesson vouched for the questions below it (as an ordinary explained lesson does in the staircase reach) the repaired program would state 22 + 12 = 24 as a *fact* (truth 26). So a repaired program records the lessons it was built around (`ProgramRule::repaired`, saved as `repaired N` / `rep <query>` lines): they are told, so the exact question is a fact, but they do not widen the range it may state as fact. Everything beyond is a labeled guess with the stated hit rate; later checks (ordinary lessons) extend the reach as before. A repaired program is not silenced by the record of the old rule's miss (the miss that made it is not its own), but still states the record's low rate.

**Numbers.** `baby invent --corrections --seeds 3`: of the 157 wrong guesses, **157 are now right guesses** (before: 157 refused); of the 137 right guesses, **137 stay right guesses** (before: 64 refused); 0 wrong facts, 0 broken. The long careful battery (`invent --careful --seeds 3 --rounds 2 --limit 4`) after 2 rounds: deceptive **518 / 202 / 0** (was 521 / 199 / 0), genuine 4,671 / 39 / 0 (unchanged), WRONG 0. **Stated facts are a wash, not a gain** (3 fewer of 720); the gain is in labeled guesses. An intermediate build read 492 / 228 / 0 (29 facts lost); the recovery is the next paragraph. (The build before the agreement guard read 678 / 42 / 0, but that build was the one that stated 240 wrong facts under a wrong teacher, so 678 was partly bought with wrong facts; I first quoted it as the result and it is not.)

**Winning the facts back (same day).** Per-kind tally against the pre-repair commit showed the whole loss in one kind, `capdouble` (min(2a, 30)), which ended as a plain rule with 13 of 27 right. Three causes, found by tracing it: (1) in a small unit (15 lessons) one contradiction is more than a rule tolerates, so the unit was re-decided and fell to *memory* before the repair ran, and with no law left there was nothing to repair: the repair now runs first, while the settled law is still in place (`learn`); (2) one point fits two equally small laws (`min(a + a, 30)` and `a + (a mod 28)`), and a program with rivals counts as open and was thrown away at the next re-decision: a repaired program is now kept with its rivals (answering only where they agree), `own_ambiguity` finds where they part ways and `check_myself`/`wonder` ask exactly there, and the answer chooses (a lesson the program merely *declined* is a test of a rival, not a contradiction, so it vouches for its neighbors like any check); (3) my own experiment harness filtered out every question except edges, so the new question was never asked in the measurement (`invent.rs`). `capdouble` is 26 of 27 again. The pre-repair build's cap of `3 x 10` from library constants let one lesson vouch for its neighbors, which was the one-point hazard (harmless there by luck), so part of the old figure was not honestly earned.

**`wrapsum`, and the box that hid it (same day).** `wrapsum` (a sum that wraps at 40) still ended with 0 stated facts of 62. Tracing it showed the repair was never attempted at the probe that exposes the wrap, (24, 22) -> 6: the unit was not `refuted`, because `refuted` still asked whether the contradicting lesson lay outside the *per-operand box* of the other lessons, and 24 and 22 had each been seen separately. It is the box-versus-staircase hole of the reach again, in a second place. `refuted` now also counts a contradiction at a point that no other lesson is at least as large as in every place (the staircase). That alone changed nothing, because the agreement guard then rejected the repair: it checked the old rule at every point of the *box*, including corner points nothing vouches for, where a wrapped law legitimately differs from a sum. The guard now checks only the points the other lessons cover (the staircase). Result: `wrapsum` 62 of 62 stated right (was 0), and the careful battery after 2 rounds: deceptive **704 / 16 / 0** (was 518 / 202 / 0, originally 521 / 199 / 0), genuine 4,671 / 39 / 0, WRONG 0; corrections 157/157 right guesses, scale/noise/permissive/teacher-check batteries unchanged. **The cost, measured:** against a *consistently* wrong teacher a little more is lost, because a wrongly-labeled corner inside the box is no longer absorbed as noise but looks like a regime change that no one can tell from the real thing: 5 % persistent 99.58 -> 99.11 % right (wrong facts 1 -> 4), 20 % persistent 87.97 -> 87.31 % (25 -> 42; 21 units refuted instead of 11); every wrong fact is still a repeat of one it was wrongly told and none is damage elsewhere; slips are unaffected (20 %: 99.97 %, 0 wrong). I judge +186 right facts of 720 on the deceptive kinds worth about half a point under a persistently wrong teacher, but it is a trade, not a free gain. Test: `a_wrap_that_shows_itself_only_at_a_corner_is_repaired_even_though_each_operand_was_seen_separately` (119 tests). Short batteries unchanged (scale sweep and 2 %/5 % noise WRONG 0.00; permissive invention 64/64, 32/36, 0/24; teacher checks 100 % / 100 %, 0 traps). Tests: 119 (was 112): saturating and wrapping sums repaired from one correction, the reflection hazard, an unrepairable pair of contradictions changes nothing, save/load of a repaired program.

**Mistakes (recorded as they happened).**
- My first test for "no repair possible" used one arbitrary correction ("24 + 12 is 7"). The repair found `(first + second) mod 29`, which fits every lesson (no taught sum reaches 29): the premise was wrong, the behavior right. A lone number is a wrap *somewhere*; it needs two contradictions that no small law shares.
- The skip lists of the reach functions are searched with `binary_search`; I appended the unproven lessons unsorted and the hull silently ignored the skip (the debug print showed the same hull with and without skipping). Sorted now.
- **The sometimes-wrong teacher run caught a real regression** that no other battery did. With a *persistent* 20 % teacher the first version had **270 wrong facts, 240 of them damage elsewhere** (before: 28, all repeats of facts it was wrongly told), 85.8 % right instead of 88.0 %. `WRONG_DEBUG` showed a *selection* unit ("which is bigger or smaller") repaired into `min(second, first + second div 6)`: a coincidental program that fits the wrongly-told lessons but disagrees with the true rule *inside* the taught range, and stated those answers as facts. A repair may extend a law beyond where it was contradicted; it must not rewrite it where it was working. Fix: the repaired program must agree with the old rule at every point (up to 30 x 30, words up to ten) of the range the other lessons cover. Re-run: persistent 20 % **87.91 % right, 28 wrong facts, all repeats, 0 elsewhere** (final build: 87.97 %, 25, 0); persistent 5 % 99.58 % (unchanged); slips 5 % 99.93 %, 20 % 99.90 %. **There is no unit test for this guard** (I could not construct a small deterministic case); the experiment is its test, so re-run `compare --teacher 200 --wrong 0.2 --persistent` after any change to repair.

**Honest limits.** Simulated teacher, tidy synthetic kinds; the language that can express the new regime is mine (add, subtract, multiply, min, max, mod, div, half and the numbers the contradictions name); a regime with more pieces than 7, or whose break is not a number the lessons name, is still only "refuted"; a law with several regime changes is explained by the smallest program that fits, which one correction cannot decide (alternatives are kept and answered only where they agree). The guard checks agreement only where it can ask the old rule (digits, or number words up to ten). The repair is attempted only on numeric units that are `refuted`; it is not yet used to *choose which check to ask next*.

## A teacher who is sometimes wrong (2026-10-22; Daws: "reinforcing based on true facts... how a human learns and trusts their new sources")

Every teacher-check result before this used a simulated teacher who is always right. `baby compare --teacher N --wrong P [--persistent] [--trusting]` makes the teacher wrong with probability P (the wrong answer is a random other number word). Two kinds of error, because they need different defenses: **independent** (a slip: asking again gets a fresh, usually right answer) and **persistent** (consistently wrong about particular questions: the same wrong answer every time, which a second question cannot catch). 200 units, seed 1, the baby's own checks as before (4 rounds, up to 4 per unit); the report counts a wrong fact as a **repeat** if it is a fact it was wrongly told (same unit, same numbers in the same order, in any phrasing) and as **damage elsewhere** otherwise.

**What a wrong answer did before any defense.** At 5 % independent errors (15 wrong answers learned), unseen and beyond-range questions went from 100 % to 87.9 % right; at 5 % persistent errors (21 learned) to **80.7 %**. No wrong facts and no refuted units: the loss was coverage. Looking at the damaged units showed why: every one was a Times or Apart program that had become "fits most but not all of what I was taught" (a partial program, which only guesses), with exactly **one exception**. One wrong lesson against dozens turned an exact program into a guess-only one.

**Two defenses.**
1. **A settled program survives a few contradicting lessons** (always on). When the refit can only offer a partial program because of at most three contradicting lessons, and at most one in twenty, the established exact program is kept (`refit_in_place`, and `relearn_unit` for a full re-decision): one lesson against dozens that agree is an outlier, not a new law (a rule already tolerated one in twenty). It states facts only where the lessons it explains reach, and a contradiction outside that reach still withholds its guesses there (the `refuted` logic of the earlier step), so a genuine change of regime stays safe: the deceptive-kinds battery is unchanged (deceptive 520 / 200 / 0, genuine 4,671 / 39 / 0).
2. **Skepticism** (**on by default since 2026-10-22**: `School::set_skeptical(false)` / `--trusting` turn it off). An answer to one of its own checks that contradicts a rule it has good evidence for (a rule, a selection or an exact program, held-back evidence above the bar, at least 8 facts, not refuted) is **held, not learned** (`Learned.disputed`); the caller asks the same question again. The same answer twice is believed (and the rule refuted properly); the rule's answer the second time means the first was a slip and it is dropped; a third version leaves it unsettled. Costs one more question per contradiction.

**Results** (right = share of the 13,384 unseen and beyond-range questions stated as fact and right; every wrong fact below is a repeat; damage elsewhere is **0 in all 16 runs**; all 467 must-refuse traps stay refused in all of them):

| errors | rate | mode | wrong answers given / learned | extra questions | questions asked | units refuted | right | refused | wrong facts (all repeats) |
|---|---|---|---|---|---|---|---|---|---|
| independent | 1 % | trusting | 5 / 5 | 0 | 528 | 0 | 99.99 % | 0.01 % | 0 |
| independent | 1 % | skeptical | 5 / **0** | 5 | 530 | 0 | **100.00 %** | 0.00 % | 0 |
| independent | 5 % | trusting | 27 / 27 | 0 | 534 | 0 | 99.01 % | 0.93 % | 8 |
| independent | 5 % | skeptical | 28 / **3** | 25 | 550 | 0 | **99.93 %** | 0.01 % | 8 |
| independent | 10 % | trusting | 66 / 66 | 0 | 551 | 0 | 99.60 % | 0.34 % | 8 |
| independent | 10 % | skeptical | 69 / **4** | 65 | 591 | 0 | **99.91 %** | 0.00 % | 12 |
| independent | 20 % | trusting | 133 / 133 | 0 | 504 | 2 | **76.77 %** | 23.06 % | 23 |
| independent | 20 % | skeptical | 165 / **10** | 153 | 685 | 0 | **99.95 %** | 0.02 % | 4 |
| persistent | 1 % | either | 8 / 8 | 0 (8) | 528 (536) | 0 | 99.85 % | 0.15 % | 0 |
| persistent | 5 % | either | 27 / 27 | 0 (24) | 536 (560) | 1 | 99.58 % | 0.41 % | 1 |
| persistent | 10 % | either | 93 / 93 | 0 (67) | 571 (638) | 5 | 97.35 % | 2.58 % | 10 |
| persistent | 20 % | either | 165 / 165 | 0 (107) | 567 (674) | 11 | **88.01 %** | 11.78 % | 28 |

(For persistent errors skeptical and trusting give identical results apart from the wasted second opinions in brackets: a repeated wrong answer cannot be told from a right one.)

**Reading it.**
- **Slips are almost free to defend against.** Skeptical learns 0 to 10 of up to 165 wrong answers for 5 to 153 extra questions (at 20 % wrong: +36 % questions) and keeps accuracy at 99.9 to 100 % up to a 20 % error rate, where a trusting baby falls to 76.8 %.
- **Consistent errors are the real limit, and only partly defensible.** Tolerance takes persistent 5 % from 80.7 to 99.6 %; at 20 % it is 88.0 % right, 11.8 % refused (11 units refuted: a contradiction beyond the reach withholds guesses) and 28 wrong facts. **Every wrong fact is a fact it was told wrongly** (and it recalls it in any phrasing, because a small unit that loses its rule falls back on memory): it is not wrong about anything it was not wrongly told, which is the honest form of the invariant here.
- **Nothing here protects against a source that is wrong about a whole kind of thing** (every sum, say): all its lessons would agree with each other. That is not tested and cannot be fixed from inside.

**Mistakes and surprises.** (1) My first tolerance change was in `relearn_unit`; the numbers did not move at all, because the new lesson comes in through the cheap refit path, which replaced the exact program by the freshly fitted partial one: I found it by printing the damaged units, not by reasoning. (2) I first counted "damage elsewhere" by the exact question text and got 6 for 5 % errors; they were other phrasings of the same wrongly-told fact (the memory recalls by words), so the accounting now keys a fact by unit and numbers. (3) A cargo test filter written as a regex ran zero tests and reported success. (4) Two tests carried a meaningless `|| true`; removed.

**Limits.** Errors are uniform over the other number words and independent of how hard the question is; one world (200 units), one seed; the 5 % / three-lessons tolerance and the 8-fact bar are mine; re-asking a person gets correlated answers more often than a fresh draw, which is what the persistent column bounds. **Decision (Daws, 2026-10-22: "go with your suggestion")**: skepticism is on by default for the baby's own checks; callers of `learn_wondered` must handle `Learned.disputed` (ask again) or use `learn_wondered_sure`. Held answers are saved in the school file. Re-run with the default on (frozen binary): independent 5 % 99.93 %, 20 % 99.95 %; persistent 5 % 99.58 %; 0 of 467 traps answered.

## The controlled comparison (Vision Entry 2): the baby against matched attention transformers (2026-10-21)

`baby compare` (`compare.rs`, `transformer.rs`) and `tools/pytorch_baseline.py`. Two kinds of transformer, both trained on the **same lessons** as the baby and graded on the **same four question sets** (vocabulary and answers only from the training lessons, plus spare slots reserved for what is taught later: a fixed network cannot represent a new word otherwise, which favors the network):
- **A plain one written from scratch in this repo** (no dependencies; pre-LayerNorm encoder, multi-head attention, ReLU feed-forward, mean-pool, Adam; the backward pass is checked against finite differences), three sizes, one fixed recipe, **tuned for none**.
- **A conventional PyTorch one** (`nn.TransformerEncoder`, CPU build 2.9.1, in a private virtual environment, not committed) with a **24-setting random search** over width, depth, learning rate, weight decay, dropout, feed-forward width and batch size. The settings are chosen **only on held-out facts of the training lessons** (all phrasings of 20 % of the facts), never on the test sets, and the cost of the search is charged to it. The baby has no search to pay for.

**How energy is measured.** The CPU package counter (Intel RAPL, made readable by Daws) counts everything the machine does, not only this process. Energy here is the package energy of a run minus the idle draw, **with the idle baseline re-measured right before every measurement** (the lowest of three one-second readings). Every run also records how much of the busy CPU time went to other processes; the tools warn when it is high (two of the Rust runs showed 29-37 %). All figures below come from one campaign on a quiet machine (load average about 1). Short windows are noisy (the baby's 0.17-second teaching read 0.25-0.33 J), so read energies to within about **plus or minus 30 %** and trust medians of repeated runs. The Rust campaign and the three PyTorch runs were measured one after another, never in parallel.

**1. Same lessons, 13-unit curriculum, seeds 1 / 2 / 3.**

| | teaching, energy above idle | never taught: right / **WRONG** | beyond the taught numbers: right / **WRONG** |
|---|---|---|---|
| **baby** | 0.25 / 0.32 / 0.33 J | 90.2 / **0**; 87.8 / **0**; 85.4 / **0** | 0 as fact, **100 % right as a labeled guess** |
| plain transformer d=16 (6k params) | 5.2 / 3.2 / 3.0 J (9-21x) | 84.1 / **15.9**; 64.0 / **36.0**; 70.7 / **29.3** | 15-18 / **82-85** |
| plain transformer d=32 (16k) | 5.3 / 3.6 / 3.3 J | 81.7 / **18.3**; 60.4 / **39.6**; 77.4 / **22.6** | 15-21 / **79-85** |
| plain transformer d=64 (49k) | 10.9 / 10.7 / 10.9 J (33-44x) | 78.7 / **21.3**; 62.8 / **37.2**; 72.6 / **27.4** | 17-20 / **80-83** |
| **PyTorch, tuned** (best of 24; 65k / 49k / 164k params) | final 155 / 506 / 57 J, **plus the search 1,515 / 1,882 / 2,973 J** | 84.8 / **15.2**; 64.6 / **35.4**; 75.0 / **25.0** | 14-20 / **80-86** |

With a 0.9 confidence cut-off a transformer is still wrong on 4.9-15 % of never-taught and 24-50 % of beyond-range questions while refusing 18-64 %. Taught and reworded questions: everyone about 100 % (the PyTorch run on seed 2 reached 98.6 / 96.6). **Tuning did not change the quality picture**: the tuned PyTorch model scored 84.8, 64.6, 75.0 on never-taught against 78.7-84.1, 60.4-64.0, 70.7-77.4 for the untuned plain ones; its chosen settings differ by seed (width 64, 64, 128) and the held-out-fact scores of the 24 settings only spanned about 32-50 %, so a bigger search would probably change little. Scaling from 6k to 49k parameters did **not** help on unseen questions: no sign of the "tiny nudge, huge gain" jump of Entry 2.

**2. Learning something new** (20 invented capitals, 60 lessons, asked in a fourth phrasing; then the old knowledge re-graded; **median of five runs**, plain transformers).

| | energy above idle | new capitals | old knowledge afterwards |
|---|---|---|---|
| **baby learns the 60 lessons** | **60 mJ** | 100 % | **unchanged exactly** (100 / 100 / 90.2) |
| fine-tune on the new only (d=16 / 32 / 64) | 125 / 142 / 227 mJ (2-4x) | 95-100 % | **forgets**: taught 100 to 49-85 %, never-taught to 37-68 % |
| new + a 10 % sample of the old | 210 / 183 / 546 mJ (3-9x) | 90-100 % | loses 3-8 points of taught/reworded, about 5 of never-taught |
| new + all the old | 1.58 / 1.22 / 3.87 J (20-64x) | 100 % | keeps everything |

**3. How the two grow** (the scale generator, trained from scratch at each size; seed 1; plain transformer d=32, one training run per size).

| units (lessons) | baby teaching | transformer d=32 | ratio | never taught right / **WRONG**: baby, transformer | beyond the taught numbers: baby (as a guess), transformer right / **WRONG** |
|---|---|---|---|---|---|
| 20 (828) | 0.22 J | 4.5 J (20 epochs) | 20x | 96.9 / **0**, 74.2 / **25.8** | 100 % right, 27.5 / **72.5** |
| 60 (2,520) | 0.81 J | 7.5 J (10) | 9x | 94.9 / **0**, 98.5 / **1.5** | 100 % right, 18.8 / **81.2** |
| 200 (8,580) | 2.5 J | 24.9 J (5) | 10x | 91.0 / **0**, 99.7 / **0.3** | 100 % right, 22.0 / **78.0** |
| 400 (17,181) | 4.95 J | 158 J (10, 71 s) | 32x | 91.6 / **0**, 99.9 / **0.1** | 100 % right, 18.7 / **81.3** |
| 600 (25,776) | 7.35 J | 1,488 J (40, 623 s) | **202x** | 89.7 / **0** (10.3 % refused), **100.0 / 0** | 100 % right, 17.7 / **82.3** |

**Repeats at 400, 600 and 1,000 units** (seeds 2 and 3 are different worlds and different network starts; plain transformer d=32):

| units | seed | baby | transformer d=32 | epochs | energy ratio | time ratio | never taught: transformer right / wrong |
|---|---|---|---|---|---|---|---|
| 400 | 1 | 4.95 J, 2.3 s | 158 J, 71 s | 10 | 32x | 31x | 99.9 / 0.1 |
| 400 | 2 | 6.11 J, 2.4 s | **invalid** (read 0.00 J), 39 s | 5 | - | 16x | 99.7 / 0.3 |
| 400 | 3 | 5.44 J, 2.3 s | 553 J, 243 s | 35 | 102x | 107x | 100.0 / 0 |
| 600 | 1 | 7.35 J, 3.5 s | 1,488 J, 623 s | 40 | 202x | 180x | 100.0 / 0 |
| 600 | 2 | 10.9 J, 3.6 s | 764 J, 481 s | 30 | 70x | 134x | 99.9 / 0.1 |
| 600 | 3 | 7.26 J, 3.5 s | 829 J, 385 s | 25 | 114x | 111x | 99.7 / 0.3 |
| 1,000 (43,023 lessons) | 1 | 14.4 J, 6.1 s | 3,659 J, 1,697 s | 45 | **253x** | **277x** | 100.0 / 0 |

The baby's never-taught accuracy at these sizes is 89.4-91.9 % right with 8.1-10.6 % refused and 0 wrong; the transformers are 15.6-19.8 % right beyond the taught range (the baby's labeled guess: 100 %). So the large gap at 400-1,000 units is **real and robust**: 16-277x in time and 32-253x in energy, always far above the 9-10x of 60-200 units; how large depends mostly on how many epochs the transformer happens to need (5 to 45). The time ratio does not depend on the energy counter and agrees with it. One reading (seed 2, 400 units) is invalid and is shown as such.

(d=64: 11.3, 23.4, 125 J at 20, 60, 200 units; 12 minutes at 400 units.) The baby's teaching cost is **flat at about 0.3 mJ per lesson** from 20 to 600 units (0.27, 0.32, 0.29, 0.29, 0.29); the transformer's per-lesson cost is not (5.4, 3.0, 2.9, 9.2, 57.7 mJ): it depends on how many epochs it needs before it answers every training lesson (20, 10, 5, 10, 40) and on how fast its embedding table grows with the vocabulary. Table rows are seed 1, one training run per size; "epochs until every lesson is right" can jump, which is why the repeats below matter.

**Why does the baby refuse unseen questions? (`baby compare --refusals 20,200,1000`.)** I had assumed careful mode makes it refuse more as the world grows (3.1 % of the never-taught questions at 20 units, seed 1, to about 10 % at 600). Printing every refusal showed the assumption was wrong:
- **It does not grow.** Over ten seeds at 20 units the refusal rate is 7.8 % on average (3.1, 6.2, 6.2, 6.2, 9.4, 6.2, 9.4, 9.4, 21.9, 0.0); over five seeds at 60 units 9.2 % (5.1, 5.1, 16.3, 7.1, 12.2); one seed each at 200, 600 and 1,000 units 9.0, 10.3, 10.6 %. Seed 1's small world was a lucky draw, and I had built an explanation on one number.
- **All of it is the reach rule.** At every size 100 % of the refusals are "a number outside the range taught" (the box) or "numbers each inside the range but not together" (the corner of the staircase); none are routing, unmet words, or a rule failing. The corner share rises (0 to 2.0, 4.4, 6.9 % of all questions at 20 to 600 units) while the box share stays at 3-5 %.
- **Where they are.** Every refused question, at 200 and at 1,000 units, lies within 0.25 range-widths of the taught range: one step above the top edge or corner of what its unit was taught. The scale generator holds out a random 20 % of each numeric unit's near facts; one that lands at the top of the range has no taught lesson at least as large in every operand. Careful mode must refuse it, because a cap or a wrap just above the taught maximum would look identical in the lessons.
- **They are not wrong.** Every refused question carried a labeled guess (200 units: 124 of 124; 1,000 units: 736 of 736) and every guess was right (these worlds contain no regime changes; the invention battery's deceptive kinds show what the same guesses cost when they do: 79.5 % right at that distance).
So the roughly 9 % is the price of "never state an unchecked top edge as a fact", paid on every holdout; it is not a defect that scales. What would reduce it, and at what cost: (a) the baby asking a teacher to confirm the corners: measured below (about 5-7 questions per numeric unit clear all of them), at the price of teacher time; (b) promoting small-distance guesses to facts, which would cost wrong facts exactly where a unit changes regime (79.5 % right at that distance in the deceptive kinds), so that is a decision for Daws about the honesty rule, not a tuning step. No code was changed for this.

**Teacher checks, automatic (`baby compare --teacher UNITS [--rounds N --limit N]`).** The baby asks its own check questions (`check_myself`, up to 4 per unit per round); a simulated teacher that knows each unit's operation (`scale::teacher_truth`, tested against every question the generator itself makes, in words and digits) answers the ones that have an answer; the baby learns them (`learn_wondered`). Number-word worlds, simulated always-right teacher, 4 rounds.

| units | seed | check questions asked (all rounds) | never taught: refused before, after | beyond the taught numbers stated as fact | wrong facts | must-refuse traps answered |
|---|---|---|---|---|---|---|
| 20 (8 numeric units) | 1 / 2 / 3 | 42 / 43 / 43 | 3.1 / 6.2 / 6.2 to **0** / 0 / 0 % | 0 to **100 %** (1,032 questions) | 0 | - |
| 200 (80 numeric) | 1 / 2 / 3 | 525 / 519 / 519 | 9.0 / 8.4 / 8.7 to **0** / 0 / 0 % | 0 to **100 %** (12,008) | 0 | 0 of 467 |
| 1,000 (400 numeric) | 1 | 2,661 | 10.6 to **1.6 %** (110 questions, all "no kind of lesson is like it": see below) | 0 to **100 %** (60,680) | 0 | 0 of 984 |

About **5 to 7 check questions per numeric unit** (all asked within the first three rounds) clear every refusal that the reach rule causes, on both the never-taught and the beyond-range questions, and **nothing it states as a fact is wrong, at any step**; the careful-mode battery with the deceptive kinds is unchanged (deceptive 520 / 200 / **0**, genuine 4,671 / 39 / **0**, against 4,668 / 42 / 0 before). This is the "ask a teacher" route working as designed, and it is cheap.

**Getting there took four fixes, each a defect the experiment exposed** (the first version of this experiment asked only 3 questions per numeric unit and cleared two thirds; the numbers above supersede it):
1. **Probes vanished when their answer could not be said.** `check_myself` skipped any probe whose predicted answer was not a number it could read back, and had no fallback: on a number line that ends at ten, 10 + 5 = 15 has no word, so every sum and product unit lost its far probes and stayed refused (Sum 1,080 of 1,080, Times 720 of 720). Now it steps back toward the edge until the probe can be asked (5 + 5, 6 + 0): unseen-question refusals fell from 2.9 % to 0.6 %.
2. **Stepping back was not enough.** "First number pushed, second at its taught maximum" overshoots for every step (6 + 5 = 11), so sums stayed refused. A new fallback probes the **frontier of what can be asked**: the farthest askable numbers that would still be refused as fact, none below another (for sums on a line ending at ten, the anti-diagonal 0+10 ... 10+0), in careful mode only. Refusals 0.6 % to 0.0 %; beyond-range 66 % to 93 % stated as fact.
3. **A program unit's per-round budget was eaten by its in-range "gap" questions**, so its frontier probe never got a turn (Apart: 800 of 2,720 stayed refused). The frontier now always gets at least two places per round.
4. **Teaching more REDUCED coverage.** With 8 questions per round the beyond-range questions were 100 % stated as fact after round 5 and **88.7 % after round 6**. A unit with a perfectly good rule (`first apart second`, 100 % held back, no exceptions) said "only taught numbers from 0 to 5" again. The reach considered only the lessons that share an "operation word" with the question, and decided what counts as one by the glue heuristic (a word in at least 40 % of the unit's facts is template glue). As check answers add facts that threshold rises, so a template word like "equals", glue at 29 facts, was no longer glue at 77, and a question containing it was judged only against the lessons that contain it, which excluded every probe lesson (they are phrased without it). Replaced by a definition that does not depend on glue: **a lesson shows where a question lies if it asks for the same function** (a rule predicts with the sum of its words' weight vectors, so every phrasing of one operation has about the same total; a selection: the same sign; a program has one function, so all lessons count). Two intermediate attempts failed and are worth recording: using the rule's `all_ops` (every word it has met, zeros included) matches every lesson; using `active_ops` (words with weight) also fails, because template words like "what" and "is" carry 0.4 of the first operand's weight each, which a merged-family test (minus not vouched for by plus lessons) caught immediately.

**What was left, a side effect it exposed, and its fix: learning erodes recognition elsewhere.** At 1,000 units the 178 never-taught questions still refused (2.6 %) were all "no kind of lesson I was taught is like this question". They are small unary units (about five facts) whose question template is shared with many other units. The right unit still wins (best unit equals true unit before and after) but its margin over the runner-up fell from **0.99 to 0.36** (0.24 after more rounds), under the recognition floor of 0.742, which was fixed when the baby was taught: as neighboring units gain lessons they gain a little on the shared words (the smoothing term V/100 lets bigger units win shared words), and a small unit's margin shrinks. So the more it learned, the more a fixed floor refused what it still correctly recognized. Hand-scaling the floor (`FLOOR_SCALE`) showed the principle was safe (0 wrong, 0 of 984 traps answered). Two attempts to make it automatic:
- **A floor kept at a fixed fraction of the margin of the baby's own lessons failed** and was reverted: the lessons it is taught keep large margins while the unseen questions of the small units collapse (the floor only moved from 0.742 to 0.578).
- **A floor kept at a fixed ratio to the leave-one-out margin works.** `Routing::scores_without` scores a lesson with its own contribution taken out of the table (mirroring `add_lesson` and `scores`), which is what an unseen question of a known unit looks like. At teaching time the ratio between the floor (set from rewordings, 0.742) and the 1st-percentile leave-one-out margin (0.50) is stored (`route_ratio`, saved with the school); afterwards `recalibrate_route` sets floor = ratio x the current leave-one-out p1, never above the teaching-time floor, over a sample of at most 2,000 lessons (about 0.1 s). It runs by itself after 200 new lessons or 2 % of the total, whichever is more (at the end of `learn` **and of `learn_wondered`**), and in `reorganize`. Measured: the leave-one-out margins and the real never-taught margins agree in the low tail (both have 1 % at 0.24), so it is a good model of unseen questions; at 1,000 units the floor falls from 0.742 to 0.356 by itself, never-taught refusals from 178 to **110 (97.4 to 98.4 % right)**, **0 wrong, 0 of 984 traps answered**; learning one new fact still costs 0.5 ms and answering is unchanged.
- **Mistakes on the way.** My first trigger (20 % growth of the total number of lessons) never fired, because the check answers added only 6 % to the total, concentrated in 400 small units that each tripled; the margins that shrink belong to the small units whose neighbors grew, so the trigger is lessons added since the last calibration. And the check answers do not come through the end of `learn` (a confirmed answer is "not surprising", `learn` returns early and `learn_wondered` adds the lesson afterwards), so the trigger had to be there too. A test that taught 234 lessons that break a unit's rule hung for minutes (each forces a full re-fit): the test now teaches lessons that fit.
- **What stays refused is by design.** The 110 left (1.6 %) are the weakest cluster, whose margin (0.24) is below the floor even after it fell: the original design recognizes about 99 % of what it is asked (it was already above the leave-one-out p1 when the baby was taught), so the weakest tail is the price of refusing mixed or unfamiliar questions. Lowering the floor further trades that for a risk of mis-routing and is a policy choice, not a fix.

**What this supports, and what it does not.**
- **Supported:** (a) on unseen questions the baby is never wrong where every transformer is wrong 0.1-40 % of the time at most sizes, tuned or not (at 600 units the transformer reaches 0 wrong); (b) the baby extrapolates (a labeled guess, right every time) where a transformer with 30 times the lessons, or a tuned PyTorch one, stays at 14-28 % right and confidently wrong the rest: extrapolation does not come from scale or tuning here, it comes from the rule-learning mechanism; (c) the baby adds knowledge without forgetting anything, while a cheap fine-tune forgets a lot and a safe one costs 3-64 times more energy; (d) teaching from scratch costs it 9-44x less than the plain transformers on the curriculum, 9-253x less across 20-1,000 units, and 2-4 orders of magnitude less than the tuned PyTorch route (final training alone 57-506 J, the search 1.5-3.0 kJ).
- **Against the simple story (kept in, on purpose):** (a) **on raw accuracy on unseen questions the transformer overtakes the baby** from 60 units on (98.5 to 100 % against 94.9 to 89.4 %), because the baby refuses about 8-11 % of unseen questions at every size (see below: that rate does **not** grow; seed 1's 3.1 % at 20 units was a low draw) while a big enough network learns the small-number tables from examples; at 600 and 1,000 units it is 100 % right and 0 wrong on unseen questions. (b) At adding a handful of facts the baby is only 2-4x cheaper than a forgetful fine-tune; what it does not do is forget. (c) **The energy ratio is not monotone**: 20x, 9x, 10x up to 200 units, then 32-253x at 400-1,000 units over three seeds; my first reading of three sizes, "the advantage shrinks with scale", was wrong and is withdrawn.
- **Caveats on the whole comparison.** (1) The curriculum is tidy and rule-shaped: the baby's home ground; a transformer with far more data, digit-aware number encoding or a larger search might do better, and 24 settings is a small search. (2) For models this small, **PyTorch on this CPU is slower and costlier than my plain Rust** (per-operation overhead on tiny batches): the final PyTorch training alone cost 57-506 J against 3-11 J for the plain ones; I had worried the Rust was the slow side, which was wrong, and a GPU would not help models this small. (3) Energy: see the method note above; about plus or minus 30 %; the baby's own teaching cost is not in the counted ledger. (4) **My counted/priced estimates (45 nm figures) were far from the measurements** (transformer teaching: 39 mJ estimated against 3-5 J measured, about 100x; the older network answering: 0.8 uJ against 27 uJ), so ratios from the estimates alone are not to be trusted. (5) One world per row except table 1; one training run per size in table 3. (6) Everything is synthetic; nothing here says what happens on real language.

**Mistakes and surprises (recorded as they happened).**
- The first gradient check failed at 7.4 % relative error: float32 round-off on gradients of about 0.001 (absolute error about 0.0001, both signs, no pattern across blocks). The test now measures the whole gradient vector (relative L2 under 3 %) and entries above 0.01 individually.
- I expected the transformer to degrade with scale; on unseen questions it **improved** (74 to 100 %). My first scale run had no beyond-range column, which would have made the transformer look better than it is; adding it is what separates the two.
- **I got the energy meter wrong three times.** (1) The first figures were one idle reading at start, subtracted for the whole run; the package counter also counted the browser and the desktop (30-70 % of the busy CPU time), so numbers were inflated unevenly. (2) I then scaled the energy by this process's share of the CPU time: it made repeated runs agree but **under-counted** (the d=16 transformer read 2.3-2.8 J against a clean 3-5 J), because package power does not split by CPU time. (3) A baseline taken once goes stale: when the browser was closed after it was measured, later readings were smaller than the stale baseline and clamped to **0.00 J** (impossible values, three runs and a 705-second one). The giveaways were a PyTorch run at 0.00 J next to runs at 2 W and 5 W for similar work. The meter now re-measures the baseline before every measurement and only flags, never rescales, a busy machine; every table above was re-measured after that. Several commit messages and earlier lines of this file quoted the superseded figures.
- **The zero reading came back after the fix**, once, in the repeat runs (seed 2, 400 units, a 38-second run): the baseline taken just before the run was higher than the draw during it because the background got quieter in between. The meter now takes the lower of the draw just before and just after, and a reading still at or below the baseline is reported as invalid (n/a), never as 0.00 J. I also found that the scale report never printed the meter's warning (only the other two reports did), so the absence of a warning in those repeat runs proves nothing about the machine being quiet; fixed. The repeats are consistent with each other (the baby's own energy at 400 and 600 units ranged 4.95-6.11 J and 7.26-10.9 J), which is the best evidence I have that they are usable.
- The tuned PyTorch model was not better than the untuned plain ones; I had half expected "just tune it" to close the gap on unseen questions.
- I claimed, from three sizes, that the baby's energy advantage was shrinking with scale and gave a reason (its cost per lesson was rising). Both were artifacts of noisy short readings; the per-lesson cost is flat and the ratio is not monotone.

## First contact with real text: SVAMP word problems (2026-10-21)

Everything before this was tidy synthetic lessons. `baby bridge` (`bridge.rs`) takes question/answer pairs somebody else wrote, says what the baby cannot read at all, teaches a share, grades the rest, and reports right / refused / **WRONG** / labeled-guess right and wrong by kind, why it refused, and every wrong fact by name. Corpus: **SVAMP** (arkilpatel/SVAMP, MIT license, `SVAMP.json`, 339,970 bytes, 1,000 one-step arithmetic word problems built so that small wording changes flip the operation); downloaded with Daws's approval to scratch space, **not committed**. Kinds come from the file (Addition, Subtraction, Multiplication, Common-Division) and are used only as the units a teacher would give.

**What it can read.** 971 of 1,000 problems (29 set aside: numbers or answers of more than four digits; decimals/negatives were rare). Only 351 have exactly two numbers; 491 have three and 155 four (distractors).

**First run: it learned nothing, and said nothing wrong.** Every unit "remember, 0 % on held-back facts"; all 296 test questions refused (261 "memorize-only unit", 35 "a word it has never met"); **0 wrong**. With units found by `discover` it shattered 971 lessons into 159 units named after story words ("jerry", "storks", "salt"): discovery assumes operation words anchor a unit, and a story has dozens of words that look just as consistent.

**Why, measured before building anything.** (1) On the two-number problems the baby's own language *can* express the relations: `first apart second` explains 152 of 174 subtraction problems, "larger divided by smaller" 90 of 91 division problems; but the program search wanted a program that fits **every** lesson, and real data is a majority plus exceptions (e.g. "458 more girls than boys, 692 girls, how many pupils" typed Subtraction). (2) A program declined any question with a word not seen in that unit, and the same check zeroed the unit's own held-back score. (3) The search gave up if any lesson held more than two numbers.

**What was built: partial programs.** When no exact program exists (and only then, so every earlier result is unchanged by construction), `ProgramRule::fit_partial_with` accepts the smallest expression that explains at least 80 % of at least 24 lessons, if it still passes an MDL test: the bits in the explained lessons minus the cost of naming each exception (log2 of the lessons plus the answer's bits) must exceed the bits of searching. A constant that fits the four-in-five of a primality table that is "no" fails it (a test). A partial program's answers are **always labeled guesses, never facts**, whose stated rate is its held-back score times the unit's **routing precision** (measured by 5-fold routing of its own lessons, only when a partial unit exists). Unmet words no longer block such a guess; a unit whose best way to learn is a partial program is taken at a guess bar of 0.6 instead of the fact bar. It is not library vocabulary, is not edge-checked like an exact rule, and is revisited whenever the library grows.

**Results (two-number problems, 3 seeds, random split; blocks split in brackets).**

| | |
|---|---|
| held-back score of the program per kind | subtraction 88 %, addition 94 %, division 92 %, multiplication 70 % (24 lessons) |
| **told each question's kind (oracle)** | guess right **67-90 %**, guess wrong 5-9 %, nothing 4-24 % |
| **routed from the words alone** | only **49-62 %** of questions reach the unit of their own kind (the biggest kind alone is 51 %) |
| labeled guesses made | 61-97 per ~100 questions; right 58-64 % of them; **wrong as a labeled guess 20-35 % of all questions** |
| stated hit rate vs realized | 55.5 vs 61.9, 55.7 vs 62.2, 64.5 vs 63.9 (blocks: 74.0 vs 64.6, 54.6 vs 58.1) |
| **answers stated as fact** | **0 right, 0 wrong** (every answer is a guess) |
| the whole file (three and four numbers included) | still nothing learned: more than 20 % of the lessons hold more than two numbers, so the tolerant search refuses; 0 wrong |

**Reading it honestly.** The way of learning is sound (oracle 67-90 %, and 50 of 51 right in the first trial). The loss is routing. (*Corrected later the same day, see the section above: the "bag-of-words ceiling" I concluded here came from the naive-Bayes-style classifiers I had prototyped; a perceptron on the same words reaches 78 %.*) A prototype of three bag-of-words classifiers (naive Bayes with and without class prior, counts of one to five) topped out at 57-65 % on the same split. The stated rates are honest in aggregate (within 1-7 points on four of five splits) and over-confident by 9 points on one blocks split. Nothing is stated as a fact, so the WRONG column the project cares about stays at 0, but **20-35 % of all questions get a wrong labeled guess**: that is what it costs to guess at all when you cannot tell which kind a question is.

**Mistakes and surprises (recorded as they happened).**
- My first idea was that number words in the stories ("one", "two") broke the operand count. Measured: 1 of 351 two-number problems. Not it.
- `--tsv` was already a boolean flag of another command and swallowed the file name; the option is `--pairs`.
- A newline inside a JSON string survived into the query; the loader now normalizes whitespace.
- **A regression the long battery caught and the short ones did not:** `baby invent --careful --seeds 3 --rounds 2` read deceptive **309 / 411 / 0** where it had read **521 / 199 / 0**. A bisect (`NO_PARTIAL`, `NO_GUESSBAR` switches, since removed) showed the partial program itself. Cause: `capsum` (a sum capped at 30) used to be a plain-memory "open" unit until the library grew a piece that let `reconsider_open` find the exact program; a partial program counts as a settled program, so it was never revisited and stayed a guess. Fix: a partial program is an open unit. Now 520 / 200 / 0 (one answer differs: `wrapsum` guesses where it used to remember).
- The 2 % label-noise row of `baby scale` reads never-taught 95.9 % (was 98.0 %): I built the previous commit separately and got the same 95.9 %, so it comes from the staircase reach of the previous step, not from partial programs. I had documented the staircase's cost at 60 and 200 units but had not measured it at 2 % noise.

**Honest limits.** Four kinds from one adversarially built corpus; the kinds are the file's labels; a second look at 3-4-number problems (which numbers matter) has not started; the stated rate uses a single routing precision per unit, not one per question; discovery on real text is unsolved (159 units); nothing was tuned on SVAMP beyond the two thresholds above (0.8 allowance, 0.6 guess bar), which are mine.

## Careful by default, labeled guesses, and what one correction does (2026-10-21; Daws's call, Vision Entry 7)

**The decision.** Daws chose `careful` as the default, and asked that it not make the baby timid: a scientist guesses, then hypothesis, theory, law; a guess that is wrong and corrected is learning, a *silent* wrong answer is the failure. So the honesty rule became: **WRONG (a fact stated and false) stays at 0; a labeled guess may be wrong; what is measured is whether guesses are calibrated and whether one correction fixes the next mistake.**

**What was built.**
1. `careful` is on by default (`--permissive` opts out; `--careful` is accepted and does nothing; a saved school records `careful 0` only when permissive, and a file with no line loads careful).
2. `Reply.guess`: beyond the range taught or checked, a trusted rule's value is offered as a *labeled guess* with its `stretch` (how far past the range, in widths of that range). It is never `answer`, so every harness still counts right/wrong on facts alone. The CLI prints `probably 42` with the reason on stderr. The scorecard reports, apart, how many of the refused had a guess and whether it was right. The must-refuse battery (negative results, unreadable numbers, unmet words, memorize-only units) gets no guess; it is a test.
3. **A miss ends the guessing out there.** Each numeric unit now records the lessons its rule does not explain (`exceptions`) and reads its reach from the lessons it *does* explain. If an unexplained lesson lies outside what the explained ones span, the unit is `refuted`: it still speaks as fact inside its explained range, it no longer guesses beyond it, and what it was told wins over the refuted rule. `learn` now keeps a confirmed guess (it used to throw it away as "nothing to learn", so a confirmed guess never extended the reach).

**Numbers (simulated always-right teacher; the invention battery).**
- Guesses by stretch (`baby invent --guesses --seeds 6`, 5,672 guesses): genuine kinds **4,921 of 4,921 right at every distance**; deceptive kinds (a rule that secretly changes regime) 79.5 % right close in, about 50 % further out. The baby cannot tell the two apart from its lessons, so a distance label is all it can say honestly.
- One correction (`baby invent --corrections --seeds 6`, one correction per deceptive kind, the wrong guess nearest the taught range): **before** this change 303 of the other 305 wrong guesses stayed wrong and **2 became WRONG facts** (the correction widened the reach of a rule it had just contradicted). **After**: 0 and 0, all 305 refused. Honest limit: the 251 *right* guesses of those kinds are refused too. It learned "my rule is wrong out there", not what the new regime is; coverage comes back with more checks.
- Recovery (`baby invent --careful --seeds 3 --rounds 2 --limit 4`): after 1 round of self-checks the deceptive kinds' wrong facts fell **96 to 4** (right 471 to 433); after 2 rounds the table is identical to the earlier one (deceptive 521 / 199 / 0, genuine 4,674 / 36 / 0). Noise batteries (`scale --noise 0.02 / 0.05`, `invent --noise 0.05`) stay at 0 wrong.
- The tidy curriculum under the new default: beyond the taught range 0 % right as fact, 100 % refused, 0 % wrong, **100 % of them offered as a correct guess**; never-taught 92.7 % right (was 95.1 %: some unseen questions have an operand past the taught range) and 7.3 % refused, 0 % wrong.

**It keeps a record of its own guessing, and states it (step 4).** Each time a guess is checked (an answer it is told for a question it would have guessed at) it counts a hit or a miss in a bucket by distance (`stretch` under 0.25, to 0.5, to 1, to 2, over 2), and saves the counts. A guess now carries `rate = (hits + 1) / (tried + 2)` and `tried`; the CLI says "my guesses this far out have been right 94 % of the time, over 120 checked", or that too few have been checked to say. **Where its guesses at that distance have been wrong more often than right it withholds them** ("so I won't guess"); a unit test builds a hostile world of eight units that all saturate and shows the fifth unit's guess withheld after four misses, while a nearer distance is still offered. This is the "boldness adapts from its own record" part, in the same spirit as the bar it sets from the noise it measures.

**Is the stated rate honest? (`baby invent --calibrate --seeds 6`: guess, be told the truth, learn, in random order over all unseen questions of the 33 kinds that have a mechanism.)** 1,436 guesses; **stated rate below the realized rate in every distance bucket** (96.8 % said vs 99.9 % right under 0.25; 87.6 vs 97.2; 86.0 vs 94.0; 77.5 vs 91.5), so it is under-confident (the prior (1, 1) makes early guesses humble), never over-confident. 8,884 questions were stated as fact in the same run, **0 of them wrong**. The withholding never triggered in this battery (hits far outnumber misses, because a miss turns a unit off after one).

**A hole this found, and its fix (the corner).** The first version of that run stated **4 wrong facts out of 9,309**, all in `wrapsum` and `capsum`. The reach was a *box* (each operand's range on its own): two confirmed guesses, (20, 2) and (2, 20), stretched it to 20 on both axes, so (18, 18) counted as "checked" though nothing near that corner was. Confirmation, which I had just made extend the reach, was the cause. The reach is now a *staircase*: a question is covered only if some explained lesson is at least as large on every operand (`Remembered::uncovered_by`); otherwise it is a labeled guess. A test pins the corner (it failed first), and the old test that expected two edge probes to vouch for (20, 15) now needs the third probe, the corner, as it should.
Cost of the stricter reach (measured, not hidden): the tidy scorecard's never-taught row 92.7 % to 90.2 % right (9.8 % refused, 4.9 % of the questions offered as a correct guess); `baby scale` never-taught 96.94 % to 94.90 % at 60 units and 90.99 % at 200 units, **WRONG 0.00 in every column** (clean and at 2 % and 5 % noise); careful recovery after 1 round: deceptive kinds' wrong facts **96 (original) to 4 to 0**, after 2 rounds deceptive 521 / 199 / 0 and genuine 4,668 / 42 / 0 (was 4,674 / 36 / 0).

**Mistakes and surprises (recorded as they happened).**
- Flipping the default made the invention battery read "hidden kinds worked out 0 of 32" with no failure anywhere: it grades beyond-range questions, which careful now refuses. It measures what can be *worked out*, so the capability batteries now run permissive explicitly and say so; `run_careful`'s first row sets permissive itself.
- I pushed one commit with a failing test (my command chain did not gate on the test result); fixed in the next commit. Commits are now gated on the tests passing.
- The first version of the correction experiment built a fresh school per kind and timed out; it now builds one per seed.
- A blanket text replace added a `guess` field to function *signatures*; the compiler caught it.
- The baseline of the correction experiment was the surprise: the 5 % allowance for unexplained lessons, built for wrong labels *inside* the taught range, absorbed a teacher's correction *outside* it as one more wrong label.
- The cross-check "rule vs the exact question taught" refused the very question the teacher had just corrected (the rule says 36, memory 30); out where the rule is refuted, what it was told now wins.

**Honest limits.**
- The teacher is simulated and always right; a real teacher is sometimes wrong, and a wrong correction now makes a unit stop guessing where it should not.
- `stretch` is a distance; the stated `rate` is a smoothed count of past checks, per distance bucket and across all units (a unit-specific or kind-specific record is not kept). It is under-confident early on by construction.
- The reach check scans the lessons that share the question's operation word (the same order of work as the box did) and is not yet counted in the operation ledger, so the "57 operations per answer" headline excludes it.
- One correction stops the mistakes; it does not teach the new regime. `check_myself` rounds win coverage back, a few points at a time.
- After `sleep --compact` the reach is read from the lessons that remain (the unexplained ones stay, the explained ones are forgotten): not tested under careful; likely too narrow, which errs safe.
- The `asked` count of the recovery experiment is 498 over 2 rounds against "about 230" over 3 rounds in the older table below; I have not found out why.

## Rules that change regime: a hole, a careful mode, and a library bug it exposed (2026-10-21)

**The hole.** Plan item: extend asking from numeric programs to rules and selections. Before building, I asked whether rules and selections can be *confidently wrong at all*. They can, in a way no earlier battery tested: a rule that fits everything taught perfectly and
changes regime outside the taught numbers. `baby invent` now has four such kinds (a sum capped at 30, a sum that wraps at 40, a double capped at 30, a triple that wraps at 50) and a control (`firstalways`, whose truth really is the first operand, which it learns as a rule and gets right). **Permissively, as before: 53 wrong answers per seed
on those four kinds** (169 over 3 seeds, 187 of 240 unseen questions right). Each unit has 100 % held-back evidence and no competing explanation in its language, so every guard passes it, and the change lies outside everything taught, so no amount of data in hand can reveal it. The project's "0 % wrong" fails on this class.

**A policy, not a patch.** Whether a rule may extrapolate is a values trade-off (its extrapolation is the project's showcase, and the thing that is wrong here), so it is a switch: `School::set_careful`, `baby ask --careful`, `baby school --careful`; **the default stayed permissive when this was written; Daws chose careful as the default on 2026-10-21** (`--permissive` is now the opt-out; `--careful` is accepted and does nothing).
Careful: a rule, selection or program answers only for operands inside the range it was taught *or has had checked* for the operation named (per-operand ranges from the memory index), and otherwise says why ("for this kind I was only taught (or have checked) numbers from 0 to 12, and this asks about 20: I won't extrapolate unchecked").

**Checks extend the reach.** `check_myself` now covers rules and selections as well as programs and asks: at most two gap questions (programs), the floor (0, if nothing near it was taught), the two numbers swapped when the order never varied, each operand pushed to about twice its largest taught value, and **the corner** (both large at once: a wrapping sum shows itself only there).
Never an answer it could not read back (over four digits). Every answer it asked for is kept even if it already predicted it, since a confirmed check is what extends the reach; a refuted one makes the unit refit.

**Result (`baby invent --careful --seeds 3 --rounds 3 --limit 4`, simulated always-right teacher; unseen = never taught + beyond the taught numbers):**

| policy | deceptive kinds right / refused / **WRONG** | genuine kinds right / refused / **WRONG** |
|---|---|---|
| permissive (as before) | 551 / 0 / **169** | 4,701 / 9 / 0 |
| careful, nothing asked | 340 / 380 / **0** | 2,224 / 2,486 / 0 |
| careful, after 1 round of checks | 471 / 153 / **96** | 3,889 / 821 / 0 |
| careful, after 2 rounds | 521 / 199 / **0** | **4,674 / 36 / 0** |
| careful, after 3 rounds | 521 / 199 / 0 | 4,306 / 404 / 0 |

Careful mode with no questions removes every wrong answer and costs more than half the genuine extrapolation; **two rounds of checks (about 230 questions over 3 seeds, a few per unit) win nearly all of it back (99.2 %) and still leave 0 wrong.** The row after one round shows the limit of a probe: a wrap that only shows at the corner of the reach box survives until the corner is probed.

**Mistakes and what they uncovered.**
- My first probes tested one operand at a time, so a wrapping sum survived the first round (96 wrong); the corner probe and a cap of two gap questions per unit (hundreds of untaught pairs had crowded the edge probes out) fixed that.
- `tenfold` taught from 1 refused a question about 0: below the smallest taught number is as unchecked as above the largest. Added a floor probe.
- **A third round made things worse, and tracing it found a library bug that has nothing to do with careful mode.** The library holds at most 8 functions but a battery has ~14 program units. Every refit of a unit re-added its program and evicted the oldest function, whose unit then refit and evicted the next, in a rotation; and because a unit is fitted with its own function excluded, it rediscovered
  itself as a *rewrite through another unit's function*, so entries grew into nested equivalents (`a^2 + b^2` became `a^2 + (a - (a + b))^2`, then deeper). Dependents then lost their function and fell back to memory. Fix: **a unit contributes a given behavior to the library once**, even after it is evicted (a test pins it); the clean battery is identical (8 seeds: 128/128 hidden, 66/72 stage-2),
  and round 3 now produces 0 wrong. It still gives back some coverage (4,306 of 4,710), because 8 slots remain fewer than the units that depend on them: open.
- I first guessed the round-3 problem was answers too long to read back. That was a real but different bug (fixed, and worth having); the evidence for the real cause came from logging each library addition and eviction.

**What this is not.** The default is permissive: out of the box the baby still answers wrongly on a rule that changes regime. A simulated, always-right teacher answers the checks. A regime change between probes (a wrap at 40 with probes at 36 and 60 ... ) can still slip through until the probe lands on it; the reach is a per-operand box.
A merged family of rule units is checked through one operator word only. Only whole numbers up to four digits.

## Tuning its own bar from what it measures (2026-10-21; Entry 5)

Plan item: let the baby set some of my constants from measurement. Two attempts, one negative.

**Negative: the search budget cannot be cut by watching progress.** Most successful program searches are cheap (median 2,223 candidates, 90th percentile 31,253) but the deepest need ~700,000, so a smaller fixed budget would lose solves. A cut-off would need a signal that separates a doomed search from a deep one. The obvious one, the best fraction of lessons any candidate explains,
does not: at ~100,000 candidates, 28 searches that went on to find a program and 29 that found nothing both sit around 0.2 (a correct deep program shows nothing until it appears). Nothing was built on it.

**Positive: the reliability bar (the 90 % a mechanism must reach on held-back facts before it may answer).** First I made it adjustable (`--bar`) and swept 0.5 to 0.95. What it does:

| bar | 10 % label noise, unseen questions answered (words / digits) | clean adversarial battery, wrong answers (4-6 seeds) |
|---|---|---|
| 0.5 | | **92** |
| 0.6, 0.7 | 100 % / 99 % | **28** |
| 0.75, 0.8, 0.85 | 96 % / 95 % at 0.8 | 0 |
| 0.9 (what it was) | **42 % / 58 %** | 0 |
| 0.95 | 13 % / 5 % (already 43-49 % at 5 % noise) | 0 |

Two facts. **The ceiling is noise:** a correct rule cannot score above about one minus the share of wrong lessons on held-back facts, so a fixed bar falls off a cliff when the world is noisier than it assumed. **The floor is real:** the wrong answers at low bars were a `select` mechanism that was right 72 % of the time on held-back facts
("remainder" explained as "the bigger or smaller of the two") and answered anyway. The noisy sweeps (3 benign seeds) had not shown that; the clean adversarial battery did, which is why it was run before anything was built.

**What it does now.** The baby measures the noise in its own lessons: lessons that say the same thing (the same content words in the same order, i.e. one fact in several phrasings) must agree, and the share that disagree with their group's vote estimates the share of wrong lessons. Its bar is
`clamp(0.98 - 2.5 x noise, 0.8, 0.9)`: exactly 0.9 for a clean world (nothing changed there), 0.8 from about 7 % noise (a margin above the 72 % that failed). The bar lives in the school (saved in the file, re-measured by `reorganize`; a thread-local while learning, so schools and parallel tests cannot interfere).
`baby scale` now prints what it believes.

**It measures well.** The baby's own estimate of the noise against the world's: 2.3 / 2.3 %, 5.5 / 5.5 %, 9.6 / 9.7 %, 14.0 / 14.2 % (words), 2.0 / 2.0, 5.1 / 5.1, 9.5 / 9.5, 14.6 / 14.9 % (digits).

| unseen questions answered, 0 % wrong on every row but the two noted | fixed 0.9 | adaptive bar |
|---|---|---|
| words, 10 % noise (60 units) | 42 % | **98.0 %** (never taught) / 99.4 % (beyond) |
| digits, 10 % noise (60 units) | 58 % | **97.0 %** / 93.7 % (0.25 % wrong on the beyond questions) |
| words, 15 % noise | | 92.9 % / 97.4 % (0.05 % wrong on taught) |
| digits, 15 % noise | | 66.7 % / 66.7 % (the floor of 0.8 is above what the data can reach) |
| 200 units: words 10 %, digits 5 % | | 93 % / 93.5 %, and 98.5 % / 98.5 %; after `reorganize` 88.8 % and 98.7 % |
| adversarial invention battery, 4 seeds, hidden / stage-2 kinds worked out; false inventions; wrong | | clean 64/64, 32/36; 5 % noise 61/64, 31/36; 10 % noise 51/64, 27/36; **0 false inventions, 0 wrong at all three** |

Clean worlds are bit-for-bit unchanged (scorecard, the clean sweep to 200 units, the clean battery).

**Honest limits.** The floor 0.8 rests on one failure (72 %) and six clean seeds at 0.75: a thin margin. At 15 % noise on digits the floor still costs a third of the coverage. The noise estimate needs groups of three or more phrasings of one fact; a world taught with single phrasings gives no estimate (it then keeps 0.9). It adapts one constant of several: folds,
the outlier allowance, the search budget and the thresholds inside voting are still mine. It measures label noise only, not ambiguity or correlated errors.

## Adapting its effort, reviewing itself, checking itself (2026-10-21)

Three changes in the direction of the learning process adapting, one of which turned out to be mostly insurance.

**1. Effort follows evidence.** Every program search had the same budget, and a unit with *no* explanation paid for six full failed searches (one on all lessons, five on cross-validation folds). The baby now searches all its lessons first, and only if that finds something does it
cross-validate: a fold that finds a program the whole data does not is a coincidence. `baby invent`: **23.5 s -> 14.0 s with identical results** (8 seeds: 128/128 hidden, 61/72 stage-2, 0 false inventions, 0 wrong). It is one rule, not a general theory of how much to try; the other knobs (folds, the 90 % bar,
the search budget, the 10 % allowance for outliers) are still constants I chose.

**2. A program that looked settled is reviewed when its vocabulary changes.** A digit sum fitted before the tens digit and the units digit existed in the library held `first mod 9` as its one explanation; later two more explanations fit equally well, but nothing told the unit. Now, once teaching is finished and whenever a lesson changes the library,
each program unit is asked one cheap question ("is my explanation still the only one that fits?", a single search that stops as soon as it has found its fits); only if not is it re-fitted. **Stage-2 kinds worked out: 61 -> 66 of 72** (8 seeds), 0 false inventions, 0 wrong, 14 -> 16 s a seed.
My first version re-fitted every program unit with full cross-validation: the same results at 76 s a seed, five times slower.

**3. A unit is never explained by vocabulary it contributed itself.** Found by a test written to reproduce the digit-sum case: the unit's own `mod 9` program had entered the library as a function, so every later search for that unit found *its own earlier invention* at size 2, where nothing could compete.
Each library function now records the unit it came from, and a unit is fitted and reviewed with the library minus its own contributions (the saved format gained an origin field).

**4. It checks its own explanations where they were tested least (`School::check_myself`, `baby wonder --check`).** For a unit that holds one program, it names the numbers nobody taught inside the range it was taught (gaps) and the first one past it, and asks about them. The unit test is the whole arc: digit sum on 0-15 with 8 and 9 missing, knowing only a ten, holds `first mod 9`, confidently says 0 at 9; it asks 8 (confirmed), 9 (refuted),
refits, then (given the tens and units pieces) sees two rivals, wonders about 19, and is right everywhere after two questions of its own choosing.

**What the experiment found (`baby invent --check`):** 105 program units asked 202 questions in 6 seeds, and **all were confirmed**: 100 % right before and after, 0 wrong. At 5 % label noise, 87 units asked 167 and all were confirmed. **The self-check never refuted anything once the review pass (change 2) existed.** An earlier version of the experiment, before the review pass, found 3 digit-sum units
that were not confirmed (100 % -> 88.9 % after checking, because it had become honest about two rivals, -> 100 % after following up with `wonder`): the review pass now catches those first. So its demonstrated value is the unit test and insurance against the data-gap case, at about two questions per program unit; I have not found a battery where it matters, and I would not claim more.

**Mistakes on the way.** I read a stale results file because it still held the previous run's `FINISHED` marker (always delete the file before a rerun). The first debug runs filtered to one kind and so did not reproduce the full run, where earlier kinds' questions change the library under the later ones (confounded, now noted). The first "refuted" counter ignored declining at the number asked.
Several runs hit my own timeouts because each seed teaches whole schools (about 100 s a seed for the check experiment); the experiments now run in the background to files.

**What this is not.** Effort adaptation is one rule. The teacher is simulated and always right. Self-checks cover numeric programs only. (The reliability bar was later made adaptive; see the section above. The fold count, the outlier allowance and the search budget are still constants.)

## It asks what it wants to know (2026-10-20; Entry 5, the learning process adapting)

Until now the baby learned only what it was handed. When the lessons left two explanations equally good (digit sum taught on 0-15: `tens + units` and `(first + tens) mod 10` agree on every taught number and part ways at 19) it declined the whole unit.
It now keeps the small set of smallest explanations that fit, **answers wherever they all agree, declines where they differ, and can say which number would settle it**. `baby wonder` prints those questions; `baby learn "<q>" <a> --unit NAME --wondered` answers one
and makes it think again at once. This is the first place the baby chooses *what to look at*, which is the part of Adaptive Learning that had no implementation.

**What was built.** `ProgramRule::alts` (the other smallest fits; saved with the school), `predict` that declines where they disagree, `ProgramRule::ambiguity` (the numbers on which the explanations give the most different answers, then the smallest), `School::wonder`,
`learn_wondered`, and `reconsider_open` (below). The old rule, "a program is trusted only if its smallest fit is unique", is now "trusted where its smallest fits agree"; a unit taught only subtraction with a >= b is the model case:
`first - second` and `first apart second` agree exactly where it was taught and part ways on negative results, which is exactly where it should decline.

**The experiment (`baby invent --active`, 6 seeds).** For each kind not already perfect, the baby asks one question at a time and a simulated teacher answers from the truth; a second teacher gives random numbers instead.

| kind | what happened |
|---|---|
| digit sum (taught on 0-15), 3 of 6 seeds where explanations compete | percent of unseen questions right: **asking 88.9 -> 100.0 after ONE question**; random numbers 88.9 -> 92.6 -> 92.6 -> 100 (three) |
| digit sum, the other 3 seeds | **no question to ask** (66.7 %, 0 wrong): the unit holds `first mod 9` as its only explanation within its vocabulary, so nothing competes; a gap that only a second explanation could expose goes unseen (a known limit) |
| four random codes, primality, xor (all 6 seeds) | **it asked nothing**, stayed at 0 % right and **0 wrong**: no competing explanations, so it invents no puzzle where there is none (the random teacher "improves" these only by memorizing one fact per number) |

**Mistakes on the way (recorded on purpose).**
- **An ambiguous program entered the library.** The unit fitted `tens + units` while `(first + tens) mod 10` was still a live alternative, `grow_library` added it as vocabulary, and the unit's own explanation was then found as a single library function, hiding its ambiguity from `wonder`. A guess had become a building block.
  Only a settled program (no competing explanation) is now vocabulary; a test pins it.
- **Cross-validation penalizes hypotheses that rest on one fact.** After the baby asked "19" and was told 10 the unit still did not solve, because any fold that held 19 out was ambiguous again (80 % held back). Keeping the alternatives (answer where they agree) removes that cliff, and also
  raised the invention battery under 2 % noise from 56/72 to 32/36 stage-2 kinds (4 seeds, 89 %).
- **Units left open were never revisited when the vocabulary grew.** Asking about one digit sum settled it and added a clean `digitsum` to the library, but a later wider digit-sum unit kept its stale state (76 %) while `wonder` searched with the new library and saw no ambiguity. A baby that gains vocabulary should look again at what it left open:
  `reconsider_open` re-fits numeric units that were left open, only when the library has actually changed. (My first version did it on every lesson and made the experiment take over 25 minutes.)

**Checks that nothing else moved:** the school and discovery scorecards, the clean sweep, the noisy scale worlds (0 wrong), the invention battery (clean 8 seeds: 128/128 hidden, 61/72 stage-2, 0 of 48 false inventions, 0 wrong; 2 % noise, 4 seeds: 62/64 and 32/36, 0 false, 0 wrong).

**What this is not.** The teacher is a simulation that always answers correctly; a wrong answer from a real teacher would be treated as one more lesson (noise handling applies, but nothing here tests it). It asks only about numeric programs, one hypothesis family (small expressions), up to eight competing hypotheses; it does not ask about rules, selections, facts or
phrasings. "Which question" is chosen by how many different answers the hypotheses give, not by expected information; there is no cost model for the teacher's time. It is not yet curiosity in the sense of exploring: it asks only when it has found two explanations and no more.

## Noise, finished: robust fitting and cross-checks (2026-10-20)

The previous noise round left honesty solved and coverage open: with 2 % wrong lessons a few rule units fell below the bar and became memorize-only, so unseen questions were refused (55-62 % right). Closing that took six changes, three of which were
wrong first.

| 60 / 200 units, never-taught right (WRONG was 0.00 % in every cell) | before | after |
|---|---|---|
| words, 2 % noise | 62 % / 56 % | **100 % / 97.7 %** |
| digits, 2 % noise | 51 % | **100 %** (and 100 % at 200 units) |
| words, 5 % noise | not measured | 96.9 % / 97.4 % |
| after `reorganize`, 200 units, 2 % noise | 85.5 % | **98.0 %** |
| invention battery, 2 % noise: hidden kinds / stage-2 kinds worked out | 46 of 96 / 12 of 54 (6 seeds) | **127 of 128 / 56 of 72** (8 seeds) |
| invention battery, 5 % noise | | 118 of 128 / 51 of 72, 0 false inventions, 0 wrong |
| clean worlds (every row of the sweep, 20 to 600 units) | 100 % | 100 % (unchanged), reorganize at 600 units 3.5 s (was 1.35 s) |

**1. Robust rule fit (`Quantity::fit_ridge_robust`).** Fit, find the lessons the rule misses by more than half a step, set the worst aside, refit; at most a tenth of the lessons, at most 12 rounds, a few per round. *First version wrong:* it protected a second rule hiding in the unit
(six "times" lessons among sums, which must stay visible so the unit is split) by requiring each suspect's words to be "explained by other lessons", but the outliers skew the fit so that *no* lesson is well explained and nothing was ever set aside (it explained 11 of 36, same as the plain fit). The
test is now relative to peers: a lesson is an outlier only if it misses by at least three times the *median* miss of the lessons sharing each of its words. Scattered mistakes qualify; every "times" lesson misses alike, so their median is as bad as they are and they stay. A test pins both.
A greedy one-at-a-time version made `reorganize` 9x slower at 600 units (a merged family of thousands of lessons re-solved hundreds of times); it now caps the rounds.

**2. Majority per input before building programs.** One corrupted phrasing of a fact gives the *same numbers with two answers*, which no program can fit, so a single wrong lesson made a kind unexplainable. The plurality of a fact's lessons decides; a tie drops the point.
(Alone, this took hidden kinds from 46 to 93 of 96 under 2 % noise.)

**3. Two witnesses.** If a rule unit's answer disagrees with what was taught *for that exact question* (voted across the phrasings of that fact), it declines ("my rule says X, but I was taught Y for this very question"). Two versions failed first: consulting memory's *similarity* match
treated a neighboring lesson as a witness and cut the clean school's never-taught score from 95.1 % to 75.6 % (the scorecard caught it); consulting only the exact lesson made a wrongly taught lesson its own single witness and refused 46 % of them. The check applies only to questions that were taught as written and uses the vote across their phrasings. Clean lessons never disagree, so it costs nothing there.

**4. A split must make things better.** Noise pushed some units just under the 0.95 evidence that triggers `split`, discovery re-clustered them badly, and one group, named after the filler word "is", became a memorize-only unit that took good lessons out of working rules. A split now needs the extracted group *and* what stays behind to be under a real mechanism, checked before anything moves.

**5. Merges use the reliability bar.** A part had to be 95 % right on held-back facts to merge, and a 15-lesson unit with one wrong lesson cannot clear that. Merge candidates and parts now use the same 90 % bar as everything else.

**6. Routing smoothing scales with the vocabulary.** The routing score penalized a unit by ln(lessons + 1) for each word it lacks, so a tiny unit that had never seen a rare word was penalized far less than the big merged family that had seen it rarely, and won the question. Additive smoothing with a vocabulary term fixes it, but the constant matters:
V/2 (textbook) broke the small clean world (reworded 100 -> 93.9 %), V/20 broke 600 clean units (96.9 % routed right), V/100 keeps both exact. **This exposed an old hidden weakness:** after `reorganize`, 600 *clean* units were only 89.7 % right on rewordings (merged families diluted their own words), and the earlier sweeps never printed coverage after reorganizing.
V/100 makes it 100 %.

**Mistakes in the harness:** the "two attributes at once" trap sometimes picked the *same* unit's attribute, a legitimate question with a stray word, and counted as an answered trap. Fixed.

**What is left.** (a) At 5 % noise some facts have *two of three* phrasings corrupted (about 0.4 % of facts); the majority is wrong and nothing can know. 7 such answers remain in the digit world (0.04-0.2 % of taught and reworded questions); unseen questions stay at 0 wrong. (b) A gap in the taught numbers that
only one explanation in the vocabulary can see is still believed: under noise the library lost the explanation that would have objected, and `first mod 9` explained a digit sum taught on 0-15 with 8 and 9 missing (9 wrong answers once, in one seed of six; it did not recur on the final code, but nothing structural prevents it). (c) Only label noise was tested (2 % and 5 %),
not ambiguous entities, misspellings that land on other words, or noise correlated across phrasings. (d) `reorganize` is 3.5 s at 600 units (1.35 s before the robust fit).

## Can the baby invent its own way to learn something? (2026-10-20)

Daws was told it would be interesting if the baby could invent mechanisms or learning processes, and left the direction to me. "Invent" needs a definition that can be tested, or it is a word: **a kind of lesson nobody wrote a
mechanism for, taught on small numbers, that the baby works out and gets right on numbers it was never shown, while kinds that have no explanation stay unexplained.** `baby invent` (`invent.rs`) is that experiment. Everything
below is on a synthetic numeric world in digits (see "Digits" at the end), 20 seeds unless said otherwise (each seed: new random codes, new splits).

**Setup.** Nine ordinary kinds are taught first (sum, minus, after, double, half, times, apart, bigger, smaller). Then 16 *hidden* kinds ("five times", "plus ten", "remainder by three", "take away but not below zero", "sum plus ten", ...),
9 *stage-2* kinds that need something the baby could only have learned from earlier kinds (a ten; a whole earlier program as a piece), and 7 kinds that **must stay unexplained**: four random codes (a random answer for each number),
primality, digit-xor, and (in the first version) digit sum. Each is taught on small numbers only (up to 15, or up to 12 of two), a fifth of the small facts are never taught, and questions on bigger numbers test it. Scored right / refused / WRONG.

**My premise was wrong, and the baseline said so.** I expected "five times", "plus ten" and the like to need new machinery. With the current mechanisms **12 of 16 hidden kinds were already solved**: the `rule` mechanism (a free coefficient per
number plus a bias) covers every linear one. What failed was exactly the four needing integer division or remainder (`modthree`, `halfdown`, `avgdown`, `remainder`), which no mechanism can express. The false-invention side was clean (0 of 7).

**Step 1: two more operations (floor-divide, remainder).** 16 of 16 hidden kinds, and the baby *composed* the constants itself: `first mod (1 + 2)`, `(first + second) div 2`. 30 seeds: 480 of 480 solved, 0 of 210 unexplainable kinds explained.
(The operations are mine; what it builds from them is not.)

**Step 2: the harder kinds found two honesty bugs that the easy ones could not.**
- **`tensdigit` (the tens digit of a number taught up to 15) was "explained" by `half(half(first - 1)) div 2` with 92 % held-back evidence, and was wrong on half the unseen numbers: 60 WRONG answers over 20 seeds.** The taught numbers have only two answers (0 and 1)
  so many expressions fit, and held-back accuracy cannot tell them apart. The project already had the principle ("when the data cannot say which one is meant, nothing should be built on the guess") for splitting but not for building. A built program
  is now trusted only if its smallest fit is unique *in behavior over 0 to 30*, not just on the taught range (60 -> 42 WRONG).
- I tried to catch what remained (a gap in the taught numbers at exactly 8 and 9, where `floor(a/8)` and the real `a div 10` part ways) with **version-space agreement**: keep near-minimal alternatives and answer only where they agree, first by enumerating
  one more size, then by small edits of the found program. Both were wrong turns. The first made every run 7x slower, the second 3 minutes 45 seconds per run and *worse* (8 of 16 hidden kinds, because legitimate programs abstained), and thinking it through the alternatives
  would not have fixed the case anyway (small edits of `floor(a/8)` mostly still say 1 at 9). Reverted. The real cause was that the true explanation needed a constant the vocabulary could not build cheaply, so a larger-constant coincidence won by Occam.
  That is what the library (next) fixes: with `10` a one-piece constant, `a div 10` (3 pieces) beats `floor(a/8)` (5).
- **Primality was "explained" in 1 seed of 20** by `(((first + half(first mod 10)) div 2) mod 2)` over thirteen 0/1 answers, with 22 WRONG answers. This is the multiple-comparisons problem: a search that tries tens of thousands of candidates will fit any pattern
  that has fewer than about log2(candidates) bits in it. The guard is information-theoretic: **the lessons must carry more bits (facts x log2 of the distinct answers) than the search spent (log2 of the candidates tried before the first fit), plus a bit.**
  A short program found early passes (`first div 10`, found among about a thousand); a long one found late over thin data does not. After it: 0 of 120 unexplainable kinds explained, 0 WRONG over 20 seeds.

**Step 3: a library, learned from experience.** Two kinds of thing the baby keeps after it has learned something, both small and bounded:
- **Constants.** An integer of at least 3 among the coefficients of a rule it learned becomes a one-piece terminal for later searches. "Plus ten" and "ten times" taught it a `10` that the base language could only make in nine pieces.
- **Functions.** A program it built for a reliable unit (not a plain operation on the two numbers) becomes callable as one piece, however large it was, and is inlined into whatever program uses it, so saved programs stay plain expressions. At most 6 constants and 8 functions
  (oldest dropped), behaviorally deduplicated, grown only from units that clear the 90 % bar, saved in the school file.

| 20 seeds | no library | with the library | library, taught in the wrong order |
|---|---|---|---|
| hidden kinds (16) worked out | 320 of 320 | 320 of 320 | 128 of 128 (8 seeds) |
| stage-2 kinds | 20 of 160 (and 60 WRONG) | **140 of 180**, every kind that is solvable at all 20 of 20 | **8 of 72** |
| unexplainable kinds explained | 0 of 120 | 0 of 120 | 0 of 48 |
| WRONG answers | 60 -> 42 -> 0 after the guards | **0** | 0 |

What it built that nobody wrote: `first div 10` and `first mod 10` (from a ten it learned in an earlier unit), `1 + (sum of squares)` and `(sum of squares) div 2` *using its own earlier program as one piece*, `(first x first) + ((first - second) x (first - second))`,
and for `avgsq` an odd but correct `1 apart (1 + avg x avg)`. **The wrong-order column is the control:** taught before the kinds that give it the ten and the sum of squares, the same kinds are not learned (only `tenplus`, a plain rule, is). So the gain is what it learned earlier, not luck or a bigger language.

**Declined on purpose: digit sum.** 0 of 20, both versions. Taught on numbers up to 15 the explanations `tens + units` and `(first + first div 10) mod 10` agree on every taught number (they part ways at 19), so the data cannot say which is meant and the baby declines.
Taught up to 25 it still declines whenever the random hold-out dropped the one number that separates them. More data, not more cleverness, is the answer, and the unit test pins both cases.

**Making it affordable** (the first library run took 112 s a battery, from 3.4 s): a work budget of one million candidates per search (checked *inside* the loops, an earlier version checked between levels and did nothing), and program building is only tried when memory, a rule and a selection did not already
explain the unit. Now 22 s for 36 units; a search that finds nothing fails cheaply. Nothing else moved: the school scorecard, discovery, the clean and noisy scale worlds (0 WRONG in words and digits at 2 % noise) are unchanged.

**What this is not.** The operations, the list of terminals, the size limit, the thresholds (90 %, one bit, one million) and what counts as "reliable" are mine; the baby builds expressions inside that language and keeps what proved useful. It does not invent a new *kind* of mechanism
(a different learner, not an expression), and it does not yet change *how* it learns (how many folds, how much to try) from its own experience: that is Entry 5's third level, and this is only its program-building part. The kinds are synthetic and tidy, numbers are whole numbers up to 30
(answers under 1,000), and 16 + 9 kinds is a small sample. It is also **order dependent by design**, like a curriculum: what it can learn tomorrow depends on what it learned today.

**Digits (the enabling change).** Numbers could only be the eleven words zero to ten, which made "times five" three facts. A digit string (up to four digits) is now a number; a question that uses digits is answered in digits; negative or oversized results are refused. A first adversarial test found a bluff immediately:
"what is -3 plus 5" answered 8, because reading words drops the sign. A number with an attached sign, decimal point, fraction slash, thousands comma or percent is now refused ("I only read plain whole numbers"), in `ask`, in the CLI (whose spelling step rebuilt the question from bare words *before* `ask` ever saw it), and when teaching.
Number words are never "glue" (see the noise section). `baby scale --digits` adds the digit world to the scale test: 60 units, 100 % right, 0 wrong, all traps refused.

## Does learning stay light when the world gets big? (2026-10-20)

Everything before this was measured on 13 units and 792 lessons. `baby scale` (`scale.rs`) generates a much larger synthetic world and measures it: 20 to 600 units (60% fact units, then rules,
selects and built expressions across nine semantics), vocabularies of 1,000 to 10,000 subjects shared across units, six overlapping phrasing styles, label noise, misspelled questions, and traps that must be refused.
`--sweep 20,60,200,600` runs each size in its own process so peak memory is honest. Same seed throughout. One laptop core.

**What the first run found** (before any change; WRONG was 0.00 % everywhere, all 467 traps refused):

| units | lessons | teach | one new fact | going back over everything (`reorganize`) |
|---|---|---|---|---|
| 20 | 828 | 4.6 s | 4.7 ms | 1.6 s |
| 60 | 2,520 | 13.2 s | 11.3 ms | 9.4 s |
| 200 | 8,580 | 41.8 s | **40.1 ms** | **99.9 s** |

Learning one lesson grew with the whole world (O(lessons): every `learn` recomputed the word weights and the routing table from every lesson, and cloned every lesson to find the next fact number), and reorganizing
was roughly O(units^2). At 200 units quality had also slipped (3 of 18 comparison units had mislabeled themselves, rewordings 97.5 %, never-taught 94.8 %) with WRONG still 0.

**What changed, each measured or tested:**

1. **Incremental tables.** `Weights::add` and a sparse `Routing` (counts per word per unit, `add_lesson`) take one more lesson in time proportional to its words. The routing score drops two terms that are identical for every unit
   (they cancel in the margin, which is all that is used) so the table no longer stores vocabulary x units floats. A test pins that incrementally kept tables are *exactly equal* to a rebuild after a mix of real operations.
   A lesson that only corrects an answer no longer rebuilds anything. Result: **a new fact costs 0.4 ms at every size from 20 to 600 units** (was 40 ms at 200). Answering also got cheaper: 85 -> 57 counted operations
   per question (159x fewer than the slow way, was 106x), same answers.
2. **Cross-validation folds that cannot be peeked through.** Memory matches by *set* of words, so the swapped twin of a commutative fact ("which is bigger two or three" / "three or two") held in another fold let `remember` look perfect
   and tie with `select`; the cheaper one won and the unit became memorize-only. Facts that share a bag of words now go in one fold. A seed-sweep test pins it (it failed on seed 10 before the fix).
3. **Stratified folds for merges.** A unit of five lessons was judged on whatever a random split left it: it could join a rule family on only 17 of 24 seeds. Each part's facts are now spread over the folds
   (24 of 24 seeds, and multiplication is rejected on 24 of 24).
4. **Merging by set, not by pair.** `reorganize` tested every pair and started over after every merge; each merge then re-ran cross-validation on the whole growing family. Profiling (not guessing) showed the merge test cost 20-30 ms
   and the re-learning *after* it 6.9 s for 564 lessons. Now a whole set of rule units is tested at once ("does one rule fit every part?"), halved only when it does not, and the merged unit is settled from the evidence that justified the merge.
   **99.9 s -> 0.32 s at 200 units, 1.35 s at 600.**
5. **The network is only tried where it can matter** (cheap mechanisms show some structure, 25 % up to the bar). It was trained and cross-validated for every unit that was a plain table and never won. Teaching 25,776 lessons:
   127 s -> **2.65 s** (792 lessons: 3.8 s -> 0.34 s). No scorecard moved.

**Clean world after the changes** (final sweep): 

| units | lessons | teach | peak MB | new fact | repeat lesson | answer | reorganize | file | mechanism right | reworded / never taught | WRONG | traps answered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 20 | 828 | 0.08 s | 5.5 | 0.38 ms | 0.65 ms | 6 us | 0.03 s | 33 KB | 100 % | 100 / 100 % | 0.00 | 0 |
| 60 | 2,520 | 0.35 s | 8.8 | 0.64 ms | 0.66 ms | 10 us | 0.09 s | 97 KB | 100 % | 100 / 100 % | 0.00 | 0 |
| 200 | 8,580 | 0.85 s | 21 | 0.41 ms | 0.61 ms | 7 us | 0.32 s | 331 KB | 100 % | 100 / 100 % | 0.00 | 0 |
| 600 | 25,776 | 2.65 s | 56 | 0.57 ms | 0.55 ms | 11 us | 1.35 s | 1,008 KB | 100 % | 100 / 100 % | 0.00 | 0 |

A 10,000-subject vocabulary (200 units) behaves the same (0.9 s to teach, 21 MB, 100 %, 0 wrong, 467 traps refused). Memory is about 2 KB per lesson including the test data; the file is 40 bytes per lesson.

**Mistakes made on the way (recorded on purpose):**

- **My first generator made rewordings that used a word the unit had never been taught** ("what is bafoba sedavubi" where "what" never appeared in that unit). The baby refused 30 % of them. I first read that as a weakness; it is the
  guard that closed the "code for five" bluff working as designed. The generator was wrong; a test now pins that every reworded question uses only words its unit was taught.
- **I attributed the 3 mislabeled units to scale.** They were a CV flaw (change 2) that scale merely made frequent: at 20 and 60 units the luck happened to hold.
- **A test failed after change 2 and I first suspected the change.** It was a test that passed on seed 1 by luck (the small unit joined the family on 17 of 24 seeds with the old folds, 18 with the new); the real flaw was
  variance in the merge test (change 3). Leave-one-out predictions were perfect, 5-fold ones were not: the clue.
- The "settled" shortcut and the network trial were tuned on a clean world; label noise (below) found them wanting.

**Label noise (2 % of taught answers wrong), in three rounds.** The first noisy run broke the headline claim; fixing it took three more findings, and the second round contained a mistake of mine.

| 2 % noise, 60 / 200 units | round 1 (clean-world code) | after the 90 % bar + settled fixes | after voting memory + order + number words |
|---|---|---|---|
| taught, right / WRONG | 91.0 / 1.87 % | 82.3 / 0.20 % | **100.0 / 0.00 %** (both sizes) |
| lessons taught *wrongly*, answered with the truth | (mislabeled, see below) | 40 % | **100 %**, repeated the wrong lesson 0 times |
| reworded, WRONG | 2.50 % | 0.83 % | **0.00 %** |
| never taught, right / WRONG | 77.3 / 6.12 % | 62.2 / 0.00 % | 62.2 and 55.5 / **0.00 %** |
| beyond the taught range, WRONG | 9.18 % | 0.00 % | **0.00 %** |
| must-refuse traps answered | 1 of 140 | 0 of 140, 1 of 467 | **0 of 140, 0 of 467** |

What each step was:

1. **A mechanism with 28 % held-back accuracy was allowed to answer.** The bar to replace memory was 25 %, which clean lessons (evidence about 1.0 or about 0) never test. It is now 90 % (`RELIABLE`; test pinned), and a reliable rule
   may leave one lesson in twenty unexplained instead of re-deciding everything (so a noisy-but-reliable unit stops costing 160 ms a lesson).
2. **Voting memory.** Memory used to answer from the single closest lesson, so one wrong phrasing out of three won. Now every lesson with the question's content words votes, weighted by closeness; the plurality answers, a tie refuses.
   Voters need only 0.3 closeness (a wordier phrasing of the same fact scores about 0.45 and must be heard) but something must be close (0.5). Wrongly taught facts are now corrected by their own other phrasings, which is redundancy the
   baby had all along.
3. **Memory ignored word order and repeated operands, so the same trap came back a fourth time.** In memorize-only numeric units "two nidovubi five" (a negative result) retrieved "five nidovubi two", and "three times three" retrieved
   "three times two" because the content words were compared as a *set*. Content words are now compared as a *sequence*, repeats kept. And "glue" (words ignored by the fill-the-blank guard) used to be any word in a tenth of a unit's
   facts, which made number words glue; it is now two fifths of the facts, and a number word is never glue ("one" is in over 40 % of the times facts and is not "what is"). Each of those was found by printing the wrong answers
   (`SCALE_WRONG=1`), not by reasoning: the examples named the cause every time.

**A mistake in my harness.** It graded the wrongly taught lessons against the answer they were *taught*, so "right" meant "repeated the wrong lesson". The first noisy tables therefore reported "40 % faithfully repeated" when the truth was
40 % repeated and 40 % refused. It now grades against the truth and counts repeats separately.

**What noise still costs is coverage, not honesty.** Never-taught and beyond-range questions are refused much more (55-62 % right instead of 100 %): a few wrong lessons skew the *exact* least-squares fit, so rule units fall to 70-89 %
held-back, miss the 90 % bar and become memorize-only, and the 3 program units that need an exact expression cannot find one. A **robust fit** (drop lessons that disagree *only if their operator word is otherwise consistent*, so scattered
noise is ignored while a polluting second rule is still caught by split) would recover that, and is the remaining noise work. Noisy units that are not settled still cost 80-250 ms a lesson (a new fact in a settled fact unit stays 0.5 ms).

**Honest limits.** Still synthetic and tidy in structure: made-up words, number words 0-10, four phrasings per fact, a vocabulary with no frequency distribution (no Zipf), no ambiguity between entities and words, no
real misspellings that land on other words. `typo` questions are all refused (none answered), so spelling robustness at scale is untested here (the letters reader exists in the older network but is not in the school). Discovery
(`baby discover`) still has its own quadratic pair loops (3.6 s on 792 lessons) and was not scaled. Teaching cost is now a property of the *generator* as much as of the baby: real text will have far more distinct units and
far less regular ones. 600 units / 26,000 lessons / 10,000 words is a long way from millions of sentences.

## Learning is light (2026-10-20)

Daws: training is enormously intensive today, and learning with Adaptive Learning should be light enough that nearly anyone can do it, with no data center. That is
testable at this scale. `baby cost` (one core of this laptop, no GPU; the CPU's energy counter is off limits here, so the measures are time and memory):

| | cost |
|---|---|
| teach it from nothing: 792 lessons, 13 units | **3.8 s, 11.9 MB** peak (3.3 MB of that is the program itself) |
| a lesson of a kind it already understands | **0.18 ms**, none of them needed learning, none kept |
| a lesson that says something it knows a new way ("added to") | **3 ms** |
| a lesson of a kind it has never seen, nobody says where it belongs | **1 ms** (the pen); the 6th lesson opened a unit that learns by a rule, 100% held back |
| going back over everything: merge and split units | 0.6 s |
| forgetting what is already explained | 0.01 s |
| the whole session | **11.9 MB** peak |

Three things make learning light here, and each was measured, not assumed:

1. **Surprise-gated.** A lesson the unit can already work out is neither learned from nor stored. Of 60 lessons of kinds it already understands, 60 cost nothing.
2. **Refit, don't re-decide.** When a surprising lesson is consistent with a unit's settled rule (a new phrasing, a new fact) the baby just refits the rule exactly and
   checks that it still explains every lesson. The expensive step, cross-validating five ways of learning, only runs when something structural changed. That took a surprising lesson
   from ~93 ms to ~3 ms.
3. **Exact where it can be.** Fitting a rule is solved, not iterated (392x fewer operations), and nothing is trained for thousands of passes.

**A bug that distorted my first numbers.** A lesson's identity was the *sorted* set of its words, so "five minus two" and "two minus five" were the same lesson, and lessons like "three plus two" after
"two plus three" were treated as corrections of each other. 39 of 60 lessons were being "corrected" instead of skipped, each one re-running the full cross-validation (93 ms each), and none were counted as kept.
A lesson's identity is now its words *in order*; the same 60 lessons now cost 0.18 ms each. (That bug could also have corrupted order-sensitive units like subtraction; it did not in the data, but would have.)

Honest limits, and they are large: 800 tidy lessons is not the world. **Nothing here says how this grows with millions of real sentences**, whether the pen and the cross-validation still stay this cheap
when lessons are noisy and overlapping, or whether a language with huge vocabulary needs more than a handful of rules. Time and memory on a laptop are an honest proxy for "needs no data center" at this scale,
and no more than that.

## Reorganizing over time: merge and split (2026-10-20)

Discovery should not be a one-off. `baby sleep` now also looks again at how the lessons are grouped, in both directions:

- **Split**: a unit that is no longer fully explained (its newer lessons stopped following one rule) is taken apart if its lessons separate into groups that each *are* explained. The largest group stays; the others
  go to the unit that builds exactly the same expression, or to a unit of their own. Test: six multiplication lessons put under `sum` by mistake were taken out and went to `times`; `sum` was itself again.
- **Merge**: two units that were taught separately merge if a rule, or a rule with a setting per word, is right for **each** of them on facts held back. Test: a unit `added` ("what is a added to b") taught on the side was folded into the rule family.

On the real school this merged the five rule-like units (sum, minus, after, double, half) into one, so there are 9 units, the same 9 that discovery found from unlabeled lessons; what the merged rule learned takes 586 bytes. Scorecard unchanged:
100% / 100% / 95.1% / 100%, 0% wrong, every nonsense question still refused.

Three bugs on the way: (1) the first split moved *every* healthy group out, including the big correct one, and left the original unit empty (the largest now stays); (2) after merging, the recognition floor was still
calibrated for 13 units and the scorecard slipped to 98%/96%/92% (all refusals) until the floor was re-set from the new table; (3) the lesson-identity bug above.

Honest limits: merging needs *both* units to have settled on a rule or a selection (a built expression is never merged on its own, the apart/minus lesson); the split needs a unit with at least ten facts, numeric lessons and a clear separation; and
neither can fix lessons the teacher got wrong in a way that still follows a rule.

## What it keeps, and forgetting what is explained (2026-10-20)

Daws raised storage: a conventional model is trained on a pile of data (tens of terabytes at the large end) and the model itself is much smaller, so the pile is
the heavy part. The baby has the same shape in miniature, and `baby storage` shows it. Of the school's 29,139 bytes, **93% is the lessons it was taught and 1.3 KB is
everything it learned from them**. By unit, the rule for sums took 87 lessons (2,780 bytes) and is 240 bytes; multiplication took 66 lessons (2,189 bytes) and is a 58-byte
program. Facts do not shrink: a unit of capitals is its lessons.

So I taught it to forget what is already explained (`baby sleep --compact`). A unit that learned a rule, a selection or an expression keeps a support set: the facts it could not get right
without, and every word it needs; a unit of facts keeps two phrasings of each. Then comes the part that took three tries:

**The promise: it must still answer every lesson it was ever taught, kept or not, and never answer wrongly.** First version: the file fell to 47% but "taught" dropped to 96.3%, rewordings to 88.6%.
Two causes. (1) I tested whether the rule still worked but not whether the baby still *recognized* the question: the router had lost examples of how each unit is phrased. Compaction now
re-checks every lesson it let go and puts back any it can no longer answer, until it can answer all of them. (2) That check covered only the lessons let go, and the recognition floor was
recalibrated from them, so the *kept* lessons of two tiny units (which have weaker routing scores) fell below it. The floor is now set from every lesson it was ever taught, with room below it.

Result on seeds 1, 2 and 3 (identical): **358 of 792 lessons kept, the file at 49% of its size (29,139 to 14,274 bytes)**, with

| questions | before | after forgetting |
|---|---:|---:|
| taught (all 792, kept or forgotten) | 100% right | **100% right** |
| reworded | 100% right | 97.7% right, 2.3% refused |
| never taught | 95.1% right | 94.5% right, 5.5% refused |
| beyond the taught range | 100% right | 99.8% right, 0.2% refused |
| **wrong, anywhere** | 0% | **0%** |

Every loss is a refusal, never a wrong answer, and the nonsense battery (22 unanswerable questions) is still all refused (a test pins both). The 2.3% lost are all one phrasing, "what is after X", the least decisive
phrasing there is: unseen phrasings are less decisive than taught ones, so they sit near the floor. A lower floor recovers some; it is the same dial as before.

Honest limits: this is a 792-lesson world. The ratio (lessons about 100x the size of what a rule learned from them) shows what the structure of *rule-like* knowledge allows; it says nothing about what 100 billion parameters
of general language would compress to, and facts (the part that grows with the world) do not compress at all. The file is plain text, with no entropy coding: a real format would shrink everything a further 3 to 10x without forgetting anything.

## Lessons that fit nothing: the pen (2026-10-20)

Pushing on Adaptive Learning: `baby learn "query" answer` no longer needs the unit named. If the baby recognizes a kind of lesson it joins that unit; if not, the lesson waits in a **pen**.
When enough gather (at least six lessons of at least five different facts) the same discovery used on a heap of unlabeled lessons looks at the pen, and a group that follows one rule
opens a unit by itself, named after the words that were new to it. A real session, nothing named:

```
$ baby learn "what is zero less than ten" ten        ... five more, "what is N less than ten"
learned: ... (this is not a kind of lesson I know yet: set aside with 4 others waiting)
the pen is empty now                                   (the sixth lesson opened a unit, 'less/than', which learns by a rule)
$ baby ask "what is eight less than ten"      two      (it was taught 0 to 5)
$ baby ask "what is ten less than three"      ?        (negative: it refuses)
```

And a new phrasing of something it already knows: six lessons "what is a multiplied by b" gathered, and at the sixth they **joined unit `times` by themselves**, because the group built the one expression
`first x second` that `times` builds. Along the way, "all N lessons waiting look like another way of asking unit 'times': confirm with --unit times" notes appear as a suggestion before it acts.

**The same trap, a third time.** My first pen joined the "N less than ten" lessons to unit `apart`, because the expression it built for them was identical to apart's. It is the minus/apart trap again:
for a number below ten, `ten - n` and `|n - ten|` agree on every lesson, so the search picked `apart` (enumerated first) and the baby would have answered "ten less than three" with 7 instead of refusing.
Two repairs, both general: an expression only counts as *the* explanation if exactly one behavior fits the lessons (two expressions that agree on every pair of numbers from 0 to 10, like `a x b` and `b x a`, are one; `second - first` and `first apart second` are two),
and the same test guards suggestions. A test pins both directions: "N less than ten" is ambiguous, multiplication is not.

Honest limits: the pen needs six lessons of five facts before it acts; it can only open a unit for a kind that one rule explains (anything else stays waiting); names are the group's new words; and joining an existing unit needs
an unambiguous identical expression, so it will *not* merge a new phrasing into a rule-family unit by itself (it suggests).

## Finding the units itself (2026-10-20)

Daws: push on Adaptive Learning, the revolutionary part, not conventional training. Until now the teacher named the unit of every lesson. `baby discover`
hands the baby **792 lessons with no grouping at all** (questions and answers, shuffled) and it works out the structure itself (`src/discover.rs`):

1. **Anchor.** Each lesson's anchor is its most telling word: the rarest word frequent enough to be a pattern, which is not a number, and which spans at
   least four different facts ("times", not "peru").
2. **Same facts, other words.** Two groups that hold the *same* facts, both ways round, are phrasings of one unit ("what does the cat say" / "cat sound",
   "double" / "doubled", "what comes after" / "after" / "then what").
3. **One rule.** Two groups of number questions merge only if a mechanism with a setting *per operation word* (a rule, a selection) is right for **each group** on facts
   held back, or both built the very same expression. "plus" and "minus" merge (a rule has a weight for each word); "times" and "plus" do not.

Result, identical on seeds 1 to 5, from 792 unlabeled lessons:

```
found unit             lessons   made of (the units the teacher would have named)
times                       66   times 66
plus/minus/equals          186   after 18, double 15, half 15, minus 51, sum 87
bigger/smaller             144   compare 144
color / opposite / capital 60/120/60   each pure
and/are                     87   apart 87
make/say/sound              42   sound 42
for/code                    27   code 27
9 units for the 13 the teacher would have named, found in 3.7 s
```

Each fact kind came out pure, the five rule-like kinds (sum, minus, after, double, half) correctly became one family with a setting per word, `compare`
needed its own mechanism, and `times` and `apart`, each of which needs an expression built for it, stayed separate. Its scorecard is
**identical to the baby the teacher organized**: right 100% / 100% / 95.1% / 100% (taught, reworded, never taught, beyond the range), refused the rest, **0% wrong**.

Three mistakes on the way, because they are the useful part:

- **"night" became an anchor.** It occurs in enough lessons (a color item and half of day/night) to look like a pattern, so the opposites split. The difference is
  spread: an operation word spans dozens of different facts, a thing being asked about spans one or two. Anchors now need at least four distinct facts.
- **The "same facts" shortcut was unsafe for numbers.** "Five minus two" and "how far apart are five and two" are both three: for the bigger number first, `a - b`
  and `|a - b|` agree on every lesson, so the two groups were merged into one unit built on `|a - b|`, and the discovered baby answered **"three minus five"
  with "two"**. The organized baby correctly refused. The scorecard had not asked that question; I found it by asking. Two repairs: the same-facts test now keeps
  the order of the numbers and must hold both ways round (a group merely *including* another's facts is a different unit); and a single built expression is not
  a license to merge, because it ignores which operation word was used. A test pins "three minus five" and "two minus nine" as refused.
- **A small group was swallowed.** Six "after" lessons joined 144 comparison lessons because the *overall* held-back score stayed above 95%: the error was
  entirely the small group. A merge now has to hold for each group separately.

Honest limits: the lessons are this tiny curated world, where numbers, shapes and phrasings are regular; the anchor test, the thresholds (4 facts, 80% number
words, 95% on held-back facts) are mine; groups with fewer than four facts cannot anchor; the names it gives units ("plus/minus/equals") are its commonest anchors and are not
pretty; it chooses among the five mechanisms and cannot invent a sixth; and "identical on five seeds" partly reflects that the structure of the world is fixed (only the
shuffle and the held-back facts change).

## A different way of computing (2026-10-20)

Daws: the mathematics can change *what* is computed or *how*, for far greater results. Fitting a rule had been done the standard way: nudge the weights
a little, 800 times over every lesson. But fitting a rule is a *linear* problem, and a linear problem has an exact answer: one small system of equations (ridge
regression, solved by elimination). No learning rate, no number of passes, the same answer every time.

| fitting the rule for sums (87 lessons) | operations | time |
|---|---:|---:|
| nudging it 800 times | ~2,505,600 | 12.3 ms |
| **solving it exactly** | **6,398** | **0.22 ms** |

**392x fewer operations, 56x faster, the same rule** (`plus` = 1.00, 1.00, 0.00; the same answers on held-back and beyond-range sums, pinned by a test). Because fitting
a rule is in the inner loop of everything (every cross-validation fold, every candidate merge in discovery), the whole thing sped up: discovery from 26-45 s to 3.7 s, teaching the school from 4.6 s to 3.9 s.
The scorecards did not move. What changed is not only speed: the iterative fit had two knobs (passes, learning rate) that were quietly part of what the baby "learned"; the
exact one has none.

Honesty: this works because the rule is linear in its weights; the built expressions (a search) and the small network are not, and are still found the slow way.
And my first attempt at swapping it in broke the file (I replaced the wrong `Mech::Select` and duplicated a third of it); it was rebuilt from the intact tail and all 55 tests pass.

## Less effort, same answers (2026-10-20)

Daws: the mathematics of the computation can make the machine do far less for results as good or better. `baby effort` counts what answering
costs (operations, 4-byte words moved, and the estimated energy of both from the 45 nm figures; absolute joules are an estimate, the
ratios are what to trust) on 2,592 questions (taught, reworded, never taught, beyond the range):

| how the baby finds and gives an answer | operations / question | words read | est. energy | right | refused | wrong |
|---|---:|---:|---:|---:|---:|---:|
| the slow way | 8,977 | 6,126 | 45.0 nJ | 99.7% | 0.3% | 0% |
| **what the baby does** | **85** | **89** | **0.80 nJ** | **99.7%** | 0.3% | **0%** |
| one small network | 7,104 | 7,104 | 388 nJ | (memorizes; cannot work out a new sum) | | |

**106x fewer operations, 69x fewer words moved, about 56x less estimated energy, the same answers.** Training dropped from 12.3 s to
4.6 s as well (one seed). Where the savings come from:

1. **Finding the unit was 99% of the cost** (8,856 of 8,977 operations): the slow way compared each question with all 792 lessons ever taught.
   The baby now keeps a table of how telling each word's *shape* is for each unit, `ln(P(word | unit) / P(word))`, and a question's score for a unit is
   the sum over its words: about 63 operations. (It is a naive-Bayes-style score; the unit with the highest score wins.)
2. **Remembered questions are found through an index of their content words**, not by scanning them all. This is safe because a question can only be
   answered from a remembered one that contains all of its content words anyway.
3. **Thrifty training**: a network is only trained and cross-validated for a unit if the cheaper mechanisms did not already reach 98% on facts
   held back; and only the mechanism it will use is fitted. Most units now never train a network at all.
4. **The answering itself was never expensive**: a learned rule is ~20 operations, an expression it built ~10. The network is 7,104 and cannot even
   do the sums.

It took two honest corrections to get "the same answers" and not just "fewer operations":
- **First version: 94.2% right** (5.8% refused). The "is this a kind of question I recognize?" check compared the raw score with a floor, and raw scores grow
  with the number of words, so a three-word question like "one plus zero" looked less recognizable than a long one, though it is unmistakably a sum. The fix
  is to measure how decisively one unit beats the next (the margin), which does not depend on length.
- **Then 99.3% (not 99.7%)**: ten questions, all one reworded phrasing ("what is after five"). The floor was set so that 95% of rewordings are
  recognized, i.e. the least decisive 5% were turned away on purpose. I made it a 99% floor and re-ran the battery of 22 unanswerable questions: all still
  refused. The floor is a dial (lower recognizes more, risks more), not a law.

Honesty about this: these are counted operations, not a meter (the CPU's energy counter is still off limits here). The comparison "one small network" is the
older single-network baby, not a clever alternative. Savings of this size come from an inefficient baseline: the first design compared against everything.

## Learning after school (2026-10-20)

The baby now keeps every lesson it has been taught, so it can go on learning. `baby learn "query" answer [--unit NAME]` adds a lesson, works out again how
that unit is best learned from *all* its lessons, and saves. With `--unit` it joins that unit or opens one of that name; without, it joins the unit the
question resembles if it recognizes a kind of lesson and refuses (saying why) if not, rather than guessing. Fewer than five facts in a unit: it only
remembers, because that is not enough evidence to trust anything else.

A real session:

```
$ baby ask "what is four multiplied by two"
?   I haven't met the words multiplied, by. If you tell me, I'll remember...
$ baby learn "what is two multiplied by three" six --unit times      (and two more examples)
learned ... (added to unit 'times'; it now has 25 facts)   it learns this unit by program (100% on facts held back)
$ baby ask "what is four multiplied by two"
eight   (unit 'times', worked out by an expression I built, (first x second) ...)

$ baby learn "what is zero less than ten" ten --unit lessthan        (a kind it has never seen: opens a new unit)
it only remembers this unit for now: with fewer than five facts there is not enough evidence to trust anything else
   ... four more lessons ...
it used to learn this unit by remember; with the new lesson it now learns it by rule (100% on facts held back)
$ baby ask "what is eight less than ten"                             (it was taught 0 to 6 only)
two   (unit 'lessthan', worked out by a learned rule ...)
```

Honest limits: the lessons are told one at a time with the unit named (it does not yet discover that several lessons belong together, or invent a unit
for lessons that fit nothing); every new lesson is treated as its own fact, so two phrasings of the same fact count as two (the cross-validation then
sees a "twin" and is a little optimistic about remembering); and the other units' mechanisms are not re-tested when a new unit changes the word statistics.

## One baby: the school (2026-10-20)

`baby school` unifies everything that worked. A curriculum of 13 **units** (capital, color, sound, opposite: facts; sum, minus, after,
double, half: arithmetic-like rules; compare: pick the bigger or smaller; times, apart: forms that had to be *built*; code: an arbitrary
mapping) is taught. The teacher groups the lessons and never says how to learn them. For each unit the baby holds back facts, tries five
mechanisms (remember, rule, select, program, network), and keeps what generalizes. A question then goes through this pipeline
(`baby ask`):

1. **What the user taught it** (`baby teach`): looked up first, any word, no weights.
2. **Spelling**: a word one or two letters from exactly one word it knows is taken to be that word, and it says so.
3. **A word it has never met**: it says so and asks to be taught.
4. **Which unit?** By a precomputed table of how telling each word's shape is for each unit (every number is a number); if no unit beats the next by enough, it says it doesn't recognize the kind of question.
5. **That unit's mechanism answers**, or declines: the rule, selection and program decline on words and shapes they have not learned; remembering
   declines unless the *word that fills the blank* matches what it was taught.

**The honest scorecard**, printed by `baby school` and identical on seeds 1 to 5 (share of questions): 

| questions | right | refused | **wrong** |
|---|---:|---:|---:|
| taught | 100% | 0% | **0%** |
| reworded | 100% | 0% | **0%** |
| never taught (held-back facts) | 95.1% | 4.9% | **0%** |
| beyond the taught range (numbers up to ten) | 100% | 0% | **0%** |

The 4.9% refused are the codes for numbers the baby was never taught: an arbitrary code cannot be known. It also refused all 27 of a
separate battery of unanswerable questions I wrote after the fact: nonsense made of familiar words ("capital of banana", "color of france"),
unrelated chat ("tell me a joke"), numbers off the line ("five times five", "three minus five"), digits ("what is 3 plus 4") and
phrasings it was not taught ("two multiplied by four"). Those last two are refused although a person could answer them: the safe failure.

**A bluff the scorecard missed, then found.** My first unified version answered "what is the code for five" with "three" (the code for
*one*), claiming it was recalled. The code unit is memorize-only, and its matching was dominated by the glue words ("code", "for"),
while number words turn up all over the curriculum and so carry little weight. The first scorecard had passed because it only asked
questions of a certain style; the check that caught it was asking for every code from zero to ten and comparing with the truth.
The fix: a word that appears across several different facts of a unit is glue; every other word of a question must match the
remembered question it is answered from. Now: 9 right, 2 refused (the two never taught), 0 wrong, and a test pins it.

Honest limits:
- **The curriculum, the units and the counting lessons are given by the teacher**, and the five mechanisms and the expression language are mine.
- **It keeps learning after `school`** (`learn`, see above), but only lessons told one at a time with the unit named.
- **The refusals are conservative**: digits, new phrasings of a kind it knows (until it is taught them with `learn`), and a negative or above-ten result are all refused.
- **"0% wrong" is on questions in or near the style it was taught.** The 27-question battery is a spot check, not a proof.

## What it is

```
query -> 8 word slots -> reader -> tanh hidden layer (64) -> softmax over 95 answer words -> one word
```

(The reader is `concat` by default, the original; `--reader bag` is smaller and generalizes better, see "Position-free readers".
The numbers in this section describe the original `concat` baby.)

- **16,271 parameters** (about 65 KB as floats; the readable text file is 185 KB). **14,272 multiply-adds per query.**
- Trains in about **6.5 seconds** on one laptop CPU core. A question takes **13 us** on this laptop (it was 32 us before one small change, below).
- Everything is hand-written: forward pass, backward pass, AdamW, the tokenizer, the RNG, the file format. Gradients are verified
  against finite differences in a test, which is the check that matters most in a from-scratch net.

## Its world

Six kinds of tiny question, each with one-word answers, in four phrasings (`src/data.rs`): color of things, animal sounds,
opposites, capitals, addition and subtraction up to 5 (number words). The data is generated in code from a seed, so every result is
reproducible. Phrasings 0-2 are for training. Phrasing 3 is a recombination of words the baby has already seen ("new-phrase"
test). For arithmetic, 15% of whole sums are held out in every phrasing ("new-sum" test: did it learn the rule or the table?).

## First results (2026-10-15)

| | train | new phrasing | new sums |
|-|-------|--------------|----------|
| shuffled, seeds 1-4 | 100% | 55 / 56 / 58 / 60 % (mean 57.3) | 19 / 6 / 25 / 44 % |
| child-like curriculum, seeds 1-4 | 100% | 61 / 69 / 62 / 62 % (mean 63.4) | 12 / 6 / 28 / 25 % |

Per family (seed 1, shuffled), new phrasing: sound 100, add 100, sub 100, color 40, capital 25, opposite 7.5.

What this teaches us, in order of confidence:

1. **It memorizes perfectly and generalizes where position lines up.** Every family that fails on the new phrasing is one where
   the key word lands in a slot it never occupied in training (read the templates in `src/data.rs`: "the opposite of X" puts X in
   slot 3; the training phrasings put it in slots 0, 2 and 5). Sound, add and sub succeed because their held-out phrasing keeps the
   words in familiar slots. This is the architecture's fault, not the data's: embeddings glued side by side tie every word to a
   position. It is a clean, checked reason to try position-free ways of reading (pooling, small convolutions, tiny attention),
   and the baby gives us a yardstick for it.
2. **It learned the table, not the rule, for sums.** Held-out sums land far below the training accuracy and swing 6-44% between
   seeds (only 32 test items, so noisy), and subtraction is the weakest.
3. **Curriculum may help a little.** Introducing the families in stages (color/sound, then opposites, then capitals, then sums,
   with the earlier ones kept in the mix) scored about 6 points higher on new phrasing across four seeds. That is a hint, not a
   finding: four seeds, noisy, and the curriculum run also takes fewer gradient steps early on. BabyLM found ordering tricks
   mostly don't help, so this deserves a bigger, fairer test before anyone believes it.
4. **Its confidence is honest.** Questions it can't answer ("capital of atlantis", "what color is a zebra", an unseen phrasing)
   come out below 0.5, and `ask` says "not sure". A free "I don't know yet", which is what a child does.

## Where this goes next (candidates, not commitments) [STALE as of 2026-10-15: the authoritative list is the last "Next" section at the bottom, and TransientHandover.md]

- **Position-free readers** compared on the same harness: does the reader matter at 16k parameters? (fixes #1)
- **Learn one thing, now:** a `teach` step that adds a new fact in a few updates without wrecking the rest, and a fact *store*
  that answers with zero updates: the first two rungs of the learning ladder (`../LEARNING.md`).
- **Letters instead of words**, so it can read words it has never seen and spell: closer to how a child starts.
- **Rules from few examples** (the sums): what inductive bias makes a tiny net learn addition instead of a lookup table?
- **A scoreboard in Transient's terms:** accuracy per parameter, per multiply-add, per byte, per second of learning.

## Thinking in joules (2026-10-15)

`src/ledger.rs` counts every multiply-add and every 4-byte memory access, then prices them with the widely cited 45 nm figures from
Horowitz (ISSCC 2014): fp32 multiply 3.7 pJ, add 0.9 pJ, a 32-bit DRAM read 640 pJ (confirmed in a search; the cache prices of 5 and
50 pJ per word are my recollection of the same talk). A modern chip spends several times less in absolute terms, so treat the joules
as an **estimate** and trust the **ratios**. When the OS allows it, `baby energy` also reads the CPU's own energy counter (Intel RAPL)
to compare the estimate with reality; this machine currently denies access (see below).

What counting showed:

- **Answering one question costs ~14,000 multiply-adds, about 0.8 microjoules (estimated), and 91% of that is memory traffic, not
  arithmetic.** Even a 63 KB network is a data-movement problem. A 7-billion-parameter model does ~500,000x more arithmetic per
  token and cannot keep its weights in cache at all, so each word it reads is priced as DRAM (640 pJ vs ~50 here).
- **One single-lesson learning step costs ~12x a question (9.9 uJ), and the optimizer is most of it.** Adam touches all 16k
  parameters and keeps two extra numbers for each, which is 70% of that step's arithmetic and two thirds of its memory traffic. In a
  batch of 16 it shrinks to ~12%. So the optimizer matters exactly when updates are small and frequent, which is what *learning*
  is. (Plain SGD is cheaper per update but needed more updates or care: no clear win without tuning; see below.)
- **Implementation is a real lever.** A dot product written as eight independent sums instead of one lets the compiler use SIMD with
  no unsafe code and no special flags: **32 us -> 13 us per question (2.5x), same network, same answers.**

To get a real measurement here (until the next reboot, run once, then `baby energy` again):

```
sudo chmod a+r /sys/class/powercap/intel-rapl:0/energy_uj
```

## Learning instead of training (2026-10-15)

`baby life` gives a newborn baby (it knows its *words*, no *facts*) the same lessons in the same order, one at a time: colors and
sounds, then opposites, capitals, sums, over two "days". It tries each question, is told the right word, and has to cope. Five ways:

| method | what it does |
|--------|--------------|
| `naive` | keep nudging the weights on the new lesson only |
| `replay` | nudge the weights on the new lesson mixed with rehearsal of old lessons (memory of 128) |
| `store` | write the lesson into a fact store (word sets weighted by rarity); never touch the weights |
| `sleep` | store while awake; now and then replay the stored lessons into the weights (mixed with older ones), then forget what the weights now know |
| `retrain` | the conventional way: freeze, then rebuild from scratch on everything seen so far at the end of each stage |

Mean of 5 seeds, Adam, lr 0.01, energy estimated (`baby life --seeds 5`):

| method | retained | new phrasing | new sums | weight updates | energy (est.) | store at end |
|--------|---------:|-------------:|---------:|---------------:|--------------:|-------------:|
| naive | 9% | 6% | 6% | 6,947 | 75 mJ | 0 |
| replay | 82% | 29% | 6% | 2,908 | 103 mJ | 0 |
| **store** | **98.5%** | **97%** | 35% | **0** | **0.39 mJ** | 285 |
| **sleep** | **98.5%** | 56% | 18% | 1,200 | **66 mJ** | 42 |
| retrain | 100% | 55% | 15% | 6,600 | 366 mJ | 0 |

What it says (with the caveats that matter):

1. **Naive learning forgets almost everything** (9% retained): each new lesson overwrites the last. The classic result, and a clean
   baseline for anything cleverer.
2. **Writing it down is ~900x cheaper than retraining and forgets nothing, on a task made of facts.** The store scores 98.5% with zero
   weight updates for 0.39 mJ against retrain's 366 mJ. It also understands rewordings (97%) because it reads *sets* of words and
   weights rare ones ("peru") over common ones ("what", "the"), which is exactly the position-free reading the weights lacked. The
   caveat is fair: this world is all lookup, so a lookup structure wins. Its limits show in sums (35% on never-taught ones, likely
   mostly from commuted twins like "two plus three" after "three plus two", which a set cannot tell apart; not yet checked) and in
   its size: it grows with experience.
3. **Sleep gets retrain's retention for about 1/5 of the energy** (66 vs 366 mJ; 98.5% vs 100%): learn into the store while awake,
   then consolidate into the weights. The store shrinks from a peak of ~60 entries to 42.
4. **Consolidation moves facts but loses the store's flexibility.** Rewording scores fall from 97% (store) to 56% (after sleep),
   the same as a batch-trained network. The knowledge moved into weights; the position-free reading did not move with it. If the
   network read words the way the store does, consolidation would keep the generalization. This is the strongest hint so far for
   what to change in the network.
5. **Replay is not enough here** (82% retained at 103 mJ): a small rehearsal buffer protects most of the past but not all, and
   it costs more than retraining per percentage point kept once sleeping is available.

Caveats, honestly: five seeds; hyper-parameters (steps per lesson, rehearsal size, sleep schedule) were set once and not tuned, so
the ranking between replay and sleep could move; the retrain baseline is a deliberately plain "start over" and a smarter conventional
pipeline would fine-tune with replay (which is what `replay`/`sleep` approximate); the energy figures are a model, not a meter;
this world is tiny and tidy.

Optimizer check (3 seeds): with plain SGD (lr 0.05-0.2) retrain needed ~338 mJ vs 366 mJ with Adam, only 8% less, because in
batches of 16 the optimizer is a small share of the work. SGD made single-lesson learning (replay) need more updates, and at low
learning rates sleeping never consolidated (the store kept everything and did the answering). So: no free win from dropping Adam
at this scale, but the *shape* of the cost (an optimizer that touches everything to learn one thing) is the target for something
smarter, such as updating only the rows a lesson actually involves.

## Position-free readers (2026-10-19)

The `life` results pointed at the reader: the store read *sets* of words and shrugged off rewording; the network glued each word to
its slot and did not. So the network now has three readers (`--reader concat|bag|bigram`, see `src/net.rs`): the original
`concat`; `bag`, the average of the words' embeddings (position-free, order-blind); and `bigram`, the bag plus hashed pairs of
neighboring words (the fastText recipe: position-free, keeps local order). Gradients of all three are checked numerically in tests.

Batch training, four seeds (`baby train --reader R`):

| reader | parameters | new phrasing | new sums | notes |
|--------|-----------:|-------------:|---------:|-------|
| concat | 16,271 | 55-60% | 6-44% | the original |
| **bag** | **9,103** | **100%** (all four seeds) | 31-50% | smaller and better |
| bigram | 18,319 | 98.6-100% | 0% | no gain over the bag here |

Learning (`baby life --reader bag`, 5 seeds, Adam; the previous concat numbers in brackets):

| method | retained | new phrasing | updates | energy (est.) |
|--------|---------:|-------------:|--------:|--------------:|
| naive | 45% (9%) | 40% (6%) | 4,429 | 26 mJ |
| replay | 97% (82%) | 94% (29%) | 2,169 | 39 mJ (103) |
| store | 98.6% (98.5%) | 97% (97%) | 0 | 0.28 mJ (0.39) |
| **sleep** | **99.2%** (98.5%) | **98%** (56%) | 720 (1,200) | **20 mJ** (66) |
| retrain | 100% | 100% (55%) | 6,600 | 187 mJ (366) |

And one question now costs **7,104 multiply-adds (about 0.4 uJ estimated; 9 us measured here; 35 KB of weights)**, half of the
original reader's 14,272 multiply-adds, 799 nJ, 13 us and 63 KB.

What it says:

1. **The reader mattered more than any learning trick.** Changing how the network *reads* made it smaller, faster, cheaper to
   teach and fixed the flaw that consolidation had: after sleeping, the weights now answer rewordings 98% of the time (were 56%).
   Sleep with the bag reader spends about **1/18 of the energy of the original retrain** (20 vs 366 mJ) for the same retention.
2. **Position-free learning is also kinder to forgetting**: naive updates retain 45% instead of 9%, and replay 97% instead of 82%,
   probably because the lessons now share parts instead of each owning its slot (a hypothesis; not isolated).
3. **Sums are still not learned.** The bag's 31-50% on never-taught sums is mostly *commutativity*: I checked, and for three of the
   four seeds it equals exactly the share of held-out additions whose swapped twin ("two plus three" after "three plus two") was in
   training, with subtraction at 0%. The bigram reader, which can tell the order, got 0%: it memorized pair patterns. At this size
   and with ~100 examples, **none of the readers learns the addition rule**. That gap (memorizing versus abstracting) is the central
   open problem for Adaptive Learning.
4. **The price is honesty about the unknown.** The old reader said "not sure" (confidence under 0.5) to "capital of atlantis". The
   bag answers *paris*, confidently, and also "what color is a zebra" -> green, "opposite of purple" -> slow. It generalizes by
   association (a lone "banana" -> yellow is a nice one), and the same association makes it guess boldly when it should abstain.
   The store and a simple rule ("a word I have never met: say so and ask to be taught") are the obvious fixes, and a good first
   test of a surprise-gated learner.

## Knowing what it doesn't know, and being taught (2026-10-19)

After the bag reader made the baby guess boldly ("capital of atlantis" -> paris at 0.73), the question became: how does it know when to
hold back? `baby know` compares three signals. Each is calibrated the same way: set its threshold so that 95% of reworded
*real* questions still get answered, then count how many *unanswerable* questions it refuses. The unanswerable ones are of two
kinds: a word it has never met ("capital of atlantis", 90+ probes) and every-word-familiar nonsense ("capital of banana", "color
of france", 100+ probes). Mean of 4 seeds:

| refuses when... | reader | answers real | right when answering | refuses unmet words | refuses nonsense |
|---|---|---:|---:|---:|---:|
| (always answers) | concat / bag | 100% | 57% / 100% | 0% | 0% |
| confidence is low | concat | 95% | 60% | 25% | 1% |
| confidence is low | bag | 95% | 100% | 100% | 99.6% |
| **question is unfamiliar** | **both** | 96% | 55% / 100% | **100%** | **100%** |
| a word is unmet | both | 100% | - | 100% | 0% |
| all three | concat / bag | 91% / 92% | 58% / 100% | 100% | 100% |

What it says:

1. **My first fix was wrong, and the study caught it.** I had `ask` refuse below 0.5 confidence. For the bag reader that floor is far
   too low: it is ~1.000 confident on real questions, so the honest threshold sits right under that. A threshold that is not
   calibrated on the baby's own experience is a guess. The baby now **calibrates its own refusal thresholds at the end of training**
   and stores them in its file (`floors` line).
2. **Familiarity beats confidence, and does not care which reader is used.** Asking "has anything like this ever been taught to me?"
   (rare-word-weighted similarity to its taught questions) refused 100% of both kinds of unanswerable question for *both* readers
   while keeping 95.6% of real ones. Confidence only works if the network is well calibrated (bag: yes after calibration;
   concat: no, 25%). The unmet-word rule is free and perfect on unmet words, and blind to nonsense (0%).
3. **Caveats.** This world is tiny and the probes are clean. Familiarity may also refuse a legitimate question made of familiar
   parts it has never seen assembled (held-out sums), which this test did not measure. Combining all three costs coverage
   (92%, not additive).

And the baby can now be **taught** (`src/memory.rs`). `baby teach "capital of atlantis" poseidonis` writes a line to
`runs/<model>.memory` and the baby answers at once: no weights change, any new word, any new answer, kept across runs in a file you
can read and edit. A real session (bag reader):

```
capital of atlantis                  => ?   I haven't met the word atlantis. If you tell me, I'll remember: baby teach ...
capital of banana                    => ?   I don't know this one (nothing I was taught is like it) ...
baby teach "capital of atlantis" poseidonis
capital of atlantis                  => poseidonis   (from what you taught me, match 1.00)
what is the capital of atlantis      => poseidonis   (match 0.74)
the atlantis capital                 => poseidonis   (match 0.77)
capital of mars                      => ?   (a different place does not retrieve it)
capital of france                    => paris   (what it knew before is untouched)
```

Limits I can already see: a filler it has never met ("the capital of atlantis *please*") counts as an unmet word and makes it refuse;
the baby cannot yet learn that "please" is noise, which is something it would learn from experience. Taught facts live in the
memory, not the network: consolidating them ("sleep" for new words and answers) needs a vocabulary that can grow.

## Letters, not words (2026-10-19)

TransientVision Entry 4: the deviation may be deeper than architecture, perhaps not even tokens. The word readers carry a fixed word
list (a small cousin of the industry's token habit). `--reader letters` has none: a question is turned into hashed pieces of its own
spelling (the whole word, 3- and 4-letter groups with word-boundary marks, pairs of neighboring words) and their embeddings are
averaged. Any word at all has a reading, including one it never met, and a misspelling keeps most of its pieces.
Gradients are checked numerically like the others (31 tests).

Misspellings: the same questions with one spelling mistake (drop, swap or replace a letter), in one word. "typos" is any word of 4+
letters; **"typo-key" is the word that carries the meaning** ("peru", "banana", "three"), which is where it matters. Mean of 3 seeds,
right answers %:

| trained with misspelled copies of each question | concat | bag | **letters** |
|---|---:|---:|---:|
| none | 11 | 10 | **37** |
| 2 copies | 14 | 12 | **44** |
| 4 copies | 14 | 12 | **48** |
| 8 copies | 15 | 12 | **61** |

(typo-key accuracy; no misspelled *word* used for testing ever appears in training, in any question). The word readers cannot learn
from misspellings they will be tested on, because each misspelling is simply a new unknown word: they stay at 10-15% however much
they see. The letters reader generalizes to spellings it never met, and improves as it meets more noise. Along the way:

- **My first explanation was wrong.** I guessed the letters reader wasn't robust because the whole-word piece always appears next
  to its letter groups so the network had no reason to lean on the groups. Removing the whole-word piece changed nothing (64-67% vs
  64-65% on any-word typos). Rejected.
- **My first augmentation result was inflated.** The same misspelled word ("pru") could appear in training in one template and in the
  test in another, which helped the word readers unfairly (bag appeared to reach 42-51%). Filtering so that test misspellings never
  appear in training dropped the bag back to 12% and left the letters reader on top. Worth remembering: this kind of leak is easy
  and invisible.
- **The cost is small.** 15,455 parameters at 512 buckets (about concat's size, not bag's); a question reads only ~50 table rows.
  In the learning experiment (`baby life --reader letters`) it matches the bag on retention and rewording (sleep: 98.9% / 97.1%)
  for about 20% more energy (25 vs 20 mJ), mostly because Adam still updates the whole table to learn one thing: sparse updates are
  the obvious next saving.
- **A spelling-aware fact store is not worth it.** Letting the store compare spelling pieces (`--piece-keys`) lifts misspelling
  accuracy from ~1% to ~19% but costs about 4.5x the energy; the network after sleeping handles misspellings far better (~54%).
  Kept as an option, not the default.

## A baby that chooses what to learn (2026-10-19)

Adaptive Learning is not only learning from experience but steering it. `baby curious`: the baby overhears 429 questions (143 facts,
each asked three ways) but not their answers, and a teacher will tell it an answer *if it asks*. How few answers does it need?

- `random`: asks about everything in random order.
- `stream`: asks only after getting a question wrong (a teacher correcting mistakes, as in `life`).
- `curious`: asks about the overheard question least like anything it has been told, and stops when nothing is. It never needs the
  teacher to judge its mistakes.
- `checked`: curious, plus doubt: when nothing seems new, it asks the teacher to confirm 10 things it believes it knows, learning from
  any it got wrong.

Accuracy on the overheard questions / on rewordings it never overheard, mean of 5 seeds, similarity needed to count as a match 0.6:

| | N=60 | N=100 | N=143 | N=200 | N=300 | asked in total |
|---|---:|---:|---:|---:|---:|---:|
| random | 34/34% | 50/52% | 64/69% | 78/83% | 92/95% | 429 |
| stream | 37/37% | 58/58% | 77/77% | 96/92% | 100/93% | 215 |
| curious | 32/41% | 55/60% | 76/77% | 93/86% | 93/86% | **197** |
| checked | 32/41% | 55/60% | 76/77% | 93/87% | 94/87% | 229 |

What it says, honestly:

1. **Choosing helps a lot against not choosing**: curious needs about half the answers random does (197 vs 429) and reaches about the
   same accuracy as a teacher-corrected stream, without anyone judging its mistakes. At a stricter match (0.7) it also generalizes
   to rewordings better per answer (73% vs 69% at 143 answers).
2. **But novelty is not correctness.** It stops at 93/86% because everything left *looks* familiar, including facts it has
   confused with similar ones. The stream baby is told when it is wrong, so it finishes at 100/93%.
3. **My idea of doubt did not work as built.** Ten random spot-checks cost 32 extra answers (229 vs 197) for about one point
   (94/87 vs 93/86). The errors it has left are rare and mostly confusable look-alikes (sums); random checking rarely lands on them.
   A better doubt would aim its checks at the places most likely to be confused (look-alike questions with different answers),
   which is an experiment worth running.
4. **Everything here depends on the match threshold** (how alike two questions must be). At 0.5 the stream baby finishes with 178
   answers; at 0.7 it needs 269. Calibrating that from experience, instead of choosing it, is part of making this adaptive.

## Rules, not tables (2026-10-19)

Every network so far memorized sums. Given "three plus two" it learned the pair, and "three plus one" was a stranger. `src/quantity.rs`
is a small unit with the two things a rule needs: a **number line**, learned the way a child gets one (taught to count: "what comes
after three? four"), and an **operator** per word, three learned numbers each, so a question's prediction is
`a*first + b*second + c` summed over its words. It is fitted on the same sums the networks see. `baby rules`, 5 seeds, right answers on sums:

| mechanism | taught | reworded | never taught (same numbers) | beyond the taught range (up to ten) |
|---|---:|---:|---:|---:|
| bag network (memorizes) | 100% | 100% | 39% | 5% |
| letters network (memorizes) | 100% | 100% | 33% | 5% |
| **quantity rule (fitted)** | 100% | 100% | **100%** | **100%** |

What the rule learned about words (nobody told it): `plus` -> (1.00, 1.00, 0.00), `minus` -> (1.00, -1.00, 0.00), and `what`, `is`,
`equals` -> (0, 0, 0): it found which words are noise, which was an open item on this list. It reaches `seven plus two` and
`nine minus four` although seven and nine never appeared in any question it was taught, because the counting lessons put them on the line.

**The baby chooses between them, from evidence it gathers itself.** While training, it holds back a fifth of the sums, learns with
both mechanisms from the rest, tests both on the held-back sums, and uses whichever does better (a tie goes to the cheaper). In every
seed: network ~41%, rule 100%, so numeric questions go to the rule. The choice and the evidence are printed and saved in the model file.

```
numeric questions: it chose the rule (on sums held back: network 11%, rule 100%)
seven plus two        => nine   (worked out by the rule, not recalled)
nine minus four       => five
what is sevn plus two => ?      I haven't met the word sevn ...        (it will not invent a missing operand)
```

Honesty about how much this is:

1. **I supplied the inductive bias.** The rule is linear in two quantities; that makes sums learnable from ~30 examples, and it is why
   nothing else here succeeds. What the baby did on its own was fit the operator, find the noise words, and decide, from held-back
   evidence, to prefer it. It did not invent the idea of quantity, and it was *taught to count first*.
2. **It chooses between mechanisms I gave it.** It cannot yet make a new one.
3. **A first version made things up.** Asked "what is sevn plus two" it answered "two" with full confidence: it had read one number
   and silently treated the other as zero. Now the rule declines unless every word is one it has met in a solved question and
   both operands are present.
4. **One rule is one rule.** Multiplication, comparison, or sequences are not covered; this is evidence that a rule *can* be learned
   from few examples with the right form, not that the baby finds forms.

## The baby adapts how it learns (2026-10-19; TransientVision Entry 5)

Entry 5: the process of learning itself should adapt. Two cases built so far, at different depths:

**How far to trust a match.** Every earlier experiment used a constant for "how alike two questions must be to trust a match" (0.6-0.75),
and results swung with it (stream baby, answers requested to finish: 178 at 0.5, 215 at 0.6, 269 at 0.7, 285 at 0.75). The `trusting`
baby learns it. Each time the teacher reveals a truth it records, per band of similarity, whether the match was right, and trusts a
band only if it was right often enough to be worth answering. "Often enough" is derived from costs: a wrong answer costs the question
that corrects it plus its own harm, so answering pays above harm/(1+harm) = 50% when harm equals one question. Across 5 seeds it
settled on **0.50 on average without being told** (the best fixed value I had found by sweeping), asked 193 answers where my hard-coded
0.75 needed 285, and ended at 100%/100%.

- **It failed first.** My first target was "right 90% of the time", which silently says a wrong answer is nine times worse than a
  question. It never moved off its starting 0.8, asked 325 answers, and answered no rewordings for the first 100. Deriving the target
  from costs fixed it. A test now pins the result across 5 seeds.

**Which mechanism to use.** The previous section: network or rule, chosen from the baby's own held-back evidence.

**Spelling.** `ask` no longer silently trips over a typo: a word it has never met that is one or two letters from exactly one word it
knows is taken to be that word, and it says so ("I took 'capitol' to mean 'capital'"). Words under five letters are left alone,
because "your" -> "four" would be a bad assumption. (The letters reader already reads some misspellings itself; this layer is
conventional, visible, and works for every reader.)

Still fixed by me and therefore not yet adaptive: steps per lesson, the sleep schedule, learning rate, the number of spot-checks.

## A library of ways to learn, chosen by evidence (2026-10-19)

`baby kinds`. Entry 5 says the mechanism of learning should adapt. Nine **units** of lessons, grouped the way a school would ("today:
comparing numbers"): `capital` (facts), `sum`, `compare` ("which is bigger three or five"), `after` ("what comes after four"), `double`,
`half`, `times`, `apart` ("how far apart are two and seven"), `code` ("what is the code for four": an arbitrary mapping with no
formula). The teacher says which lessons belong together and **never how to learn them**. Five mechanisms (`src/mechanisms.rs`):
**remember** (write questions down, answer with the most alike), **rule** (the quantity unit), **select** (bigger or smaller of two
numbers), **program** (search for an expression that reproduces every taught answer, see below), **network** (the bag net on that unit).
For each unit the baby holds back facts, learns with each mechanism from the rest, tests all five on the held-back facts (a
cross-validation it runs itself), and uses the best. A tie within two points goes to the cheaper one, and a mechanism must show
at least 25% on held-back facts to replace plain memory. Questions are sent to a unit by shape (every number counts as "a number").
Mean of 3 seeds, held-back accuracy %:

```
unit       remember   rule  select  program  network   chose
capital          0%     0%      0%       0%       0%   remember 3/3   <- nothing here generalizes; it can only memorize this
sum             57%   100%     26%     100%      57%   rule 3/3
compare         83%    21%    100%       0%      86%   select 3/3
after            0%   100%      0%     100%       0%   rule 3/3
double           0%   100%      0%     100%       0%   rule 3/3
half             0%   100%      0%     100%       0%   rule 3/3
times           62%     5%     19%     100%      62%   program 3/3
apart           55%     0%      2%     100%      56%   program 3/3
code             0%     4%      0%       0%       0%   remember 3/3   <- nothing here generalizes
```

| system (each unit counting equally) | reworded | new items | beyond what it was taught |
|---|---:|---:|---:|
| **the baby's own choices** | **100%** | **88%** | **100%** |
| remember everywhere | 100% | 34% | 6% |
| rule everywhere | 48% | 53% | 43% |
| select everywhere | 18% | 20% | 35% |
| program everywhere | 67% | 75% | 80% |
| network everywhere | 100% | 36% | 16% |

The 88% on new items is `code` scoring 0%: an arbitrary code cannot be known for a number never taught, and the baby says so. Every
other unit scores 100% on all three tests, and 100% of questions went to the right unit.

What it says:

1. **Nothing in the code says which mechanism suits which unit**, and every unit is treated alike. Facts go to remember, arithmetic-like
   units to the rule, comparison to select, and two kinds that the earlier four could not express (`times`, `apart`) to **program**.
   No single mechanism everywhere comes close on new and beyond-range questions.
2. **It builds a mechanism, not only chooses one.** For `times` the search wrote `(first x second)`; for `apart`, `(first apart
   second)`; for `after`, `(first + 1)`; for `half`, `half(first)`. Before `program` existed, `times` and `apart` were flagged as
   memorize-only (answers beyond what it was taught: 33% and 4%); now 100%.
3. **It did not "explain" what has no explanation.** On `code` (a fixed random-looking mapping) and `capital` the search found
   nothing, and the held-back test would have rejected a coincidence anyway.
4. **I took the convenient operation away to check it was not handed the answer.** `apart` is a primitive in the search. Without it
   (add, subtract, multiply, max and min only) it composed `((first - second) max (second - first))`: the absolute difference,
   built from subtraction and max, not the `max - min` I had predicted. My test expectation was wrong, the search was right.
5. **Two bugs the table exposed.** The small-fit default: with nothing above 25% the baby "chose" the rule for `code` on 4% noise
   and could no longer answer even the facts it was taught (0% on rewordings); the safe default is to remember. And the misrouting
   and confounded-credit fixes from the earlier version of this section, kept below.

Honesty about how much this is:
- **The language is mine.** The search composes add, subtract, multiply, max, min, absolute difference, halve, the first and second
  numbers, and the constants 1 and 2, up to seven pieces. It finds forms inside that space. Division with remainders, three
  numbers, or conditions are outside it; a unit needing them would be flagged memorize-only, as `times` was before.
- **The number line is taught**, and the units are given by the teacher.
- **Search cost grows fast with size.** The size limit of seven is what keeps the search instant (a few milliseconds per unit here).
- **"Beyond what it was taught" works only when the baby has seen the numbers somewhere** (the counting lessons put six to ten on the line).

The road there before `program`, because the dead ends are the useful part:
- **First run:** 80% on rewordings, all from `after` (0% on "what is after four"). I blamed the word "is" being unseen and
  let common words through: nothing changed.
- **Real cause:** the data cannot say whether "what" or "after" does the work (two of the three phrasings contain both), and a
  minimum-effort fit gave "what" a third of the credit. The fix is a prior: a word that turns up all over everything else is a poor
  suspect (pulled toward zero), and an operation word belongs to one kind of lesson.
- **First fix over-corrected:** I defined "all over" by frequency, which called "plus" common (there are many sums) and killed sums
  (0%). The right notion is **spread across units**, not frequency.
- **Misrouting (5%):** "zero plus six" went to `after`, because "six" happened to occur only in that unit's lessons. Matching by
  **shape** (every number is just a number) fixed it: 100% routing.

## Where the baby stands as Adaptive Learning (2026-10-19)

| AL ingredient | in the baby today |
|---|---|
| learns from interaction, not a fixed dataset | yes, in small: `teach`, `life`, `curious` |
| internals change in real time | yes: the store instantly; the weights during `sleep` |
| doesn't learn what it already knows | yes: only ~1/3 to 1/2 of lessons trigger learning |
| knows what it doesn't know | yes (familiarity, calibrated thresholds, "?" with a reason; flags units where nothing generalizes) |
| chooses what to learn | partly: `curious` chooses what to ask, not what to explore |
| not tied to a token list | partly: the letters reader and spelling assumptions; answers still come from a fixed list |
| one system, honest about its limits | **yes**: `baby school`/`ask` run units, mechanisms and refusals together; 0% wrong on the scorecard, with a stated list of what it refuses |
| builds abstractions (rules, not tables) | **yes, within a small language**: it builds expressions for forms no fixed mechanism covers (`times`, `apart`) and they reach beyond what it was taught; the language of pieces and the number line are supplied |
| the learning process itself adapts | **more**: it learns how far to trust a match, picks the mechanism per kind of lesson from its own held-back evidence (new kinds included, no code), and builds a mechanism when none fits; steps, schedule and rate are still fixed |
| discovers on its own (browsing, other modalities) | **no**: a teacher answers; nothing explores |
| grows (a bare UMA becoming a learned UMA) | **no**: capacity is fixed up front |

## en-US (2026-10-23)

Everything in the repo is now en-US: prose, comments, identifiers (`memorize_only`, `reorganize_inner`, `behaviors`) and the strings the program prints. It is a standard for the rebuild too: Novice is en-US from its first line.

It touched baby's code, so it was checked rather than assumed. All 142 tests still pass, and `baby school` prints the same table apart from the spelling in two lines and the run time. One coupling to remember: the unit-explanation text "I only memorize this kind of thing" is written in `school.rs` and matched in `compare.rs`, so a spelling change to one has to reach the other. Saved `runs/` models carry none of the renamed strings, but older logs in `runs/` use the old spelling. Quote punctuation (commas and periods outside closing quotes) was not changed.

## Next (candidates; the freedom to reorder is mine)

1. ~~Does learning stay light when the world gets bigger?~~ Answered for clean lessons up to 600 units (see the scale section; learning one lesson is flat at ~0.5 ms). Label noise is solved at 2 % and 5 % (robust fitting, cross-checks); open: other noise kinds, a Zipf-shaped vocabulary, discovery at scale, and the pen with thousands of waiting lessons.
2. **Compress what it keeps without forgetting anything**: a compact binary format, and the phrasings of a fact stored as templates plus facts.
3. **Incremental and exact updates**, sparse updates, small integers, and the real energy meter to replace time and memory as the measure.
4. **Let it propose its own mechanisms** when nothing generalizes (the units that stay "memorize only").
5. ~~Digits~~ done; ~~a richer space to build rules in~~ done (floor-divide, remainder, a learned library). It now also asks questions (see "It asks what it wants to know"). Still open: discover the number line without counting lessons; mechanisms of a different *kind* (not an expression); the baby adapting its own numeric knobs (folds, search effort, thresholds) from what it measures; asking about rules, selections and facts.
