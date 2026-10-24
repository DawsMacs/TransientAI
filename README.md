<p align="center">
  <img src="assets/banner.svg#gh-dark-mode-only" alt="Transient" width="69%">
  <img src="assets/banner-light.svg#gh-light-mode-only" alt="Transient" width="69%">
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: AGPL-3.0-only" src="https://img.shields.io/badge/license-AGPL--3.0--only-blue"></a>
  <a href="https://github.com/DawsMacs/Transient/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/DawsMacs/Transient/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="Rust, zero dependencies" src="https://img.shields.io/badge/rust-zero%20dependencies-orange">
</p>

**Transient is an experimental learning system written in Rust, built to run on ordinary consumer hardware.** It explores
*adaptive learning*: a system that keeps learning while it is used, decides for itself *how* to learn each kind of thing, and is
measured by what it costs (operations, memory, estimated joules) rather than by parameter count alone.

The current artifact is `baby/`: about 17k lines of Rust, **no dependencies**, builds offline, 149 tests. It is a research prototype
evaluated mostly on a synthetic curriculum, with a first look at real word problems (SVAMP, ASDiv) and an entry exam that any
other system can take. It is not a language model and makes no claim to be one yet.

Transient is made by **[TransientAI](https://transientai.org)**, an independent research effort (currently one person, Daws Macs). Its
website has an overview, documentation, a progress page and a blog.

> **Status:** active research. Unless a line says otherwise, the numbers below come from tidy synthetic lessons; energy is
> *measured* with the CPU's own counter where stated, and *estimated* from counted operations where not. Read
> [Limitations](#limitations) before drawing conclusions.

## What `baby` does

1. **Learns from units of lessons.** A unit is a group of related question/answer pairs (capitals, sums, "which is bigger", ...).
   The teacher can group them, or `baby discover` finds the units itself from an unlabeled heap.
2. **Chooses how to learn each unit, from evidence.** For every unit it picks among five mechanisms using cross-validation on facts
   it holds back, preferring the cheaper on ties:

   | mechanism | what it does | typical use |
   |---|---|---|
   | `remember` | store the lesson, answer by weighted similarity (inverted index) | facts: capitals, colors, codes |
   | `rule` | exact ridge fit of a per-word linear combination of quantities | sums, differences, doubling, halving |
   | `select` | pick the larger or smaller of two numbers | comparisons |
   | `program` | enumerative search over small expressions (`+ - x max min apart half`, up to 7 pieces) | multiplication, "how far apart" |
   | `network` | small bag-of-words net | rarely needed |

3. **Routes questions with a precomputed table** of how telling each word's *shape* is for each unit (numbers count as "a number"),
   then lets that unit's mechanism answer or decline.
4. **Explains every answer and every refusal.** It says how it knew, and refuses (with a reason) on unmet words, unrecognized
   kinds, memorize-only units it was not told the fact for, and mechanism-level declines (negative results, out-of-line numbers,
   missing operands).
   Past the range it was taught or has had checked it does not state a value as fact: it offers a *labeled guess* ("probably 42"),
   keeps a record of how often its guesses at each distance turn out right and says so, and stops guessing where they have been
   wrong more often than right. A teacher's correction ends its guessing in the region it contradicted.
5. **Keeps learning after training.** A surprise gate skips lessons it can already work out; consistent lessons refit a unit in
   place; unrecognized lessons wait in a *pen* and open a new unit once enough gather; `baby sleep` merges units that follow one
   rule, splits units that stopped following one, and can forget lessons that are fully explained (`--compact`).

## Measurements

All on the synthetic curriculum in `baby/src/kinds.rs` (13 units, 792 lessons), one laptop core, no GPU.

| claim | result |
|---|---|
| scorecard: taught / reworded / never taught / beyond range (careful by default) | stated as fact 100 / 100 / 90.2 / 0 %, **0 % wrong** on every row; the rest refused, and every beyond-range question (100 %) refused with a right labeled guess |
| answering effort | 8,977 -> 57 operations per question (159x fewer); about 85x less *estimated* energy; same answers |
| fitting a rule | exact solve vs 800-pass SGD: 392x fewer operations, ~60x faster, same rule |
| teaching 792 lessons from nothing | 0.34 s, 11.9 MB peak |
| a lesson it already understands / a new phrasing / a new kind | 0.19 ms (not stored) / 0.4 ms / 1 ms (a unit opens at the 6th) |
| **scale** (clean synthetic world, 20 -> 600 units, 828 -> 25,776 lessons, up to 10,000-word vocabulary) | teaching 26k lessons 2.65 s and 56 MB; learning one new fact **0.4-0.6 ms at every size**; answering ~10 us; reorganizing 1.35 s; 100 % right, **0 % wrong**, all traps refused |
| **inventing mechanisms** (16 hidden + 9 stage-2 numeric kinds in digits, 20 seeds) | all 16 hidden kinds worked out every time; 7 of 9 stage-2 kinds every time using constants and programs it learned earlier (taught in the wrong order: 8 of 72); 0 of 120 unexplainable kinds explained; 0 wrong; digit sum declined as ambiguous |
| **asking** (digit sum taught on 0-15, 6 seeds) | where two explanations fit, it asks the number that separates them: 100 % after one question where random numbers need three; it asks nothing about random codes, primes or xor (0 wrong) |
| **rules that change regime beyond what was taught** (a sum that saturates or wraps) | permissively (`--permissive`) 53 wrong answers per seed; **careful, the default since 2026-10-21** (answer as fact only within the range taught or checked) removes them and costs half the extrapolation; two rounds of self-checks win back 99 % with 0 wrong |
| **its own reliability bar** | measured from the noise in its lessons (it estimates 9.6 % where the world has 9.7 %) and set just under what a correct rule can reach; at 10 % label noise unseen questions answered 98 % instead of 42 %; clean worlds unchanged |
| **label noise** (2 % and 5 % of taught answers wrong, 60-200 units, words and digits) | **0 % wrong** on unseen questions; at 2 % essentially as good as clean data (97-100 % of unseen questions right); wrongly taught lessons answered with the truth by voting memory, robust rule fitting and cross-checks |
| **the entry exam** (`baby exam`, 60 units, 2,520 lessons, 4,832 questions) | taught 100 %, reworded 100 %, never seen 94.9 % as facts + 5.1 % right guesses, beyond range 100 % right guesses; **0 wrong facts, 0 of 140 traps answered**; teaching 0.68 J and answering 0.28 J (59 uJ per question), measured with the CPU's counter |
| **the adaptive exam** (787 questions on rule-changing kinds, the truth told after each answer) | **0 wrong facts**; the rule-changing kinds climb 90.6 -> 93.8 -> 100 % right across the stream; 268 J to answer and adapt (0.34 J per told answer, down from 1.80 J). A naive Python memorizer scores 0 right at 0.08 J |
| **after a correction** | the law is rebuilt from the numbers the correcting lessons name: 157 of 157 wrong guesses became right guesses after one correction (they were refused before); the law it replaced is kept as a competing explanation until two later lessons separate them |
| **real word problems** (SVAMP, ASDiv; private data, not in this repo) | labeled guesses only, never stated as fact: two-number problems about 56 % right on SVAMP (35-73 % across six splits; fragile) and 58-60 % on ASDiv; problems with more numbers than the rules use 27-38 %. Which operation the wording means is the ceiling, not a shortage of lessons |
| storage | school file 29 KB, of which 1.3 KB is what was learned; `--compact` keeps 49 % of the file, still answers all 792 taught lessons, 0 % wrong |
| unit discovery from 792 unlabeled, shuffled lessons | 9 units found (13 named by the teacher), scorecard identical |

The scorecard reports **WRONG separately from refused**: a refusal is an acceptable answer, a bluff is not.

## Quick start

Requires a stable Rust toolchain (edition 2021). No other dependencies, no network.

```bash
cargo test --release --offline         # 149 tests (about 21 s)
cargo build --release --offline
./target/release/baby school           # teach 13 units (~4 s), print how each was learned and the scorecard
./target/release/baby ask "how far apart are two and nine"   # answers, and says how it knew
./target/release/baby ask "three minus five"                 # refuses: negative result
./target/release/baby discover --out runs/discovered.school  # find the units from unlabeled lessons
./target/release/baby cost             # time and memory of learning
./target/release/baby scale --sweep 20,60,200,600   # does learning stay light as the world grows? (about 5 min)
./target/release/baby invent --seeds 20            # can it work out kinds of lesson nobody wrote a mechanism for? (about 8 min)
./target/release/baby wonder [--check]             # what would it like to be told? (--check: where would it test its own explanations?)
./target/release/baby tutor                        # it asks what it wants checked and you answer (Enter skips, q quits; `question => answer` teaches it something of your own)
./target/release/baby exam write /tmp/exam --units 60 && ./target/release/baby exam run /tmp/exam   # the entry exam: plain files, judged on wrong facts, traps, honest confidence and joules
```

`baby exam` is built so that any other system, in any language, can take it: `exam write DIR` writes plain files, `exam score DIR
predictions.tsv` judges any predictions, and an adaptive tier (`exam adapt-write`, `adapt-drive`) speaks a small line protocol over
standard input and output. `tools/exam_baseline.py` and `tools/exam_adapt_baseline.py` are naive systems to compare against.

If the scorecard's WRONG column is ever above 0.0 %, or a refusal battery in `school.rs` starts answering, treat it as a
regression: honesty is the property this project protects first.

## Platforms

Written in portable Rust with no dependencies and no `unsafe`. **CI-tested on GitHub's runners** (virtual machines running the real operating systems): Linux x86_64 (glibc and musl), Linux aarch64, macOS arm64 and x86_64, Windows x86_64, and FreeBSD, NetBSD and OpenBSD (in virtual machines). On all of them the tests pass, the scorecard reads 0.0 % wrong, and the exam and adaptive-stream answers (`golden/`) are byte-identical to the Linux x86_64 reference, although the platforms' math libraries differ in the last bit (`cargo run --release --offline --example mathprint`). That is a result of testing, not a promise of support: other operating systems, versions and hardware are untested (DragonFly BSD was tried: the tests pass but its packaged Rust crashes the binary at startup). **Measured joules need Linux on hardware with a readable RAPL counter** (not a VM or container); elsewhere the energy report says it is unavailable. Details and the open questions: [`baby/README.md`](baby/README.md), "Running on other platforms".

## Code map

All under `baby/src/`.

| file | role |
|---|---|
| `invent.rs` | the invention experiment: hidden kinds nobody wrote a mechanism for, and kinds that must stay unexplained |
| `scale.rs` | the scale test: a generator of large synthetic worlds (units, vocabulary, noise, traps) and the measurements |
| `school.rs` | the unified system: units, routing table, recognition floor, `ask`, `learn`, pen, `reorganize`, `compact`, save/load |
| `mechanisms.rs` | the five mechanisms, cross-validation, mechanism choice, the inverted-index memory |
| `discover.rs` | unit discovery from unlabeled lessons (anchor words, fact and rule-coherence merges) |
| `program.rs` | expression search and ambiguity detection (by behavior, not by taught examples) |
| `quantity.rs` | number line, per-word operator weights, exact ridge fit |
| `bridge.rs` | `baby bridge`: real-text corpora (SVAMP JSON, ASDiv XML or question/answer TSV) taught, graded and explained; a minimal JSON reader |
| `selector.rs` | on real text, which numbers in a question matter (learned from the baby's own programs) |
| `exam.rs` | the entry exam: static and adaptive tiers, a line protocol, and the judge, independent of the baby's internals |
| `tutor.rs` | `baby tutor`: the baby asks, a person answers |
| `transformer.rs`, `compare.rs` | `baby compare`: a from-scratch attention transformer (hand-written backward pass) and the controlled comparison against the baby, with measured energy; `compare --export DIR` writes the lessons for other tools |
| `kinds.rs` | the synthetic curriculum |
| `ledger.rs` | counted operations and words moved, priced with published 45 nm figures; optional RAPL reader |
| `memory.rs`, `store.rs` | user-taught open-vocabulary facts and the weighted-Jaccard fact store |
| `net.rs`, `model.rs`, `data.rs`, `know.rs`, `life.rs`, `curious.rs`, `rules.rs`, `rng.rs` | the earlier from-scratch network and its experiments, plumbing |
| `main.rs` | the CLI (`baby <command>`) |
| `tools/pytorch_baseline.py` | (outside the Rust build) a tuned PyTorch transformer trained and graded on the exported lessons, with measured energy |

## How results are judged

- **WRONG is reported apart from refused.** Accuracy alone hides bluffing.
- **Adversarial batteries are part of the tests.** The same trap (`|a-b|`, `a-b` and `10-n` agree on every taught lesson) caught
  the system three times; ambiguity is now defined by behavior, merges are checked per group, and operand order is kept.
- **Energy is measured where stated and otherwise estimated.** The entry exam and `baby compare` read the CPU's own counter (Intel
  RAPL, above an idle baseline; it needs a one-time permission change on the host, and readings are upper bounds on a busy machine).
  The older operation counts times published per-operation costs are estimates: trust the ratios, not the absolute joules.
- **Dead ends are written down.** `baby/README.md` records wrong hypotheses and bugs next to the results.

### Against matched transformers (`baby compare`, `tools/pytorch_baseline.py`)

Two transformers are trained on the same lessons and graded on the same questions: a plain one written from scratch in this repo (pure Rust, hand-written backward pass, checked against finite differences), and a conventional PyTorch one tuned by a 24-setting search whose cost is counted against it. Energy is **measured** with the CPU's own counter (Intel RAPL, above an idle baseline re-measured before every run, on a quiet machine; read it to within about plus or minus 30 %). On the synthetic curriculum (seeds 1-3; full tables, the three mistakes I made measuring energy, and every caveat in `baby/README.md`):

- teaching from scratch costs the baby 0.25-0.33 J, the plain transformers 3-11 J, and the tuned PyTorch route 57-506 J for the final training plus 1.5-3.0 kJ for the search; adding 20 new facts costs it about 60 mJ with nothing forgotten, against 125-227 mJ for a fine-tune that forgets heavily, 183-546 mJ for one that replays a tenth of the old (and still loses a few points) and 1.2-3.9 J for a full replay;
- on unseen questions the baby is wrong 0 % of the time against 15-40 % for the small transformers, tuned or not; beyond the taught numbers the baby's labeled guess is right every time where the transformers are 14-28 % right and confidently wrong the rest, at every scale tried (20 to 1,000 units);
- across 20 to 1,000 units the baby's teaching cost stays at about 0.3 mJ per lesson (43,023 lessons in 6 s); the transformer's does not: the energy ratio is 20x, 9x, 10x at 20, 60, 200 units and 32-253x at 400-1,000 units (three seeds; it depends on how many epochs the transformer needs, 5 to 45);
- **against the simple story:** from 60 units on the plain transformer is *more* accurate than the baby on unseen questions (98.5 to 100 % against 94.9 to 89.4 %; at 1,000 units it is 100 % right with 0 wrong), because the baby refuses about 8-11 % of unseen questions at every size (all of them one step beyond the top edge of what a unit was taught, each with a right labeled guess; the apparent rise from 3 % at 20 units was one lucky world); for a handful of new facts the baby is only 2-4x cheaper than a forgetful fine-tune, it just does not forget;
- **asking a teacher works:** about 5-7 automatic check questions per numeric unit (the baby asks its own, a simulated teacher answers) clear every refusal the careful reach rule causes: at 200 units 100 % of unseen and 100 % of 12,008 beyond-range questions are stated as fact (0 wrong, 0 of 467 must-refuse traps answered), at 1,000 units 98.4 % and 100 % (the weakest 1.6 % tail is what the recognition floor is designed to leave; it now follows the margins as the baby learns); simulated always-right teacher;
- **a teacher who is sometimes wrong:** an exact program now survives a few contradicting lessons instead of dropping to a guessing one, and a skeptical mode (on by default) holds an answer that contradicts a settled rule until it is repeated. With independent slips up to 20 % it keeps 99.9-100 % of unseen and beyond-range questions right (a trusting baby falls to 76.8 % at 20 %); with a teacher consistently wrong about particular things (the hard case) it keeps 99.6 % at 5 % and 88 % at 20 %, and every wrong fact it states is a fact it was wrongly told, never damage elsewhere (simulated errors; `baby/README.md`);
- caveats: the lessons are tidy and rule-shaped (the baby's home ground); 24 settings is a small search; my earlier counted energy estimates were about 100x below the measurements; for models this small PyTorch on a CPU was slower and costlier than my plain Rust.

## Limitations

- Lessons are synthetic and regular (numbers 0-10, a handful of templates). "0 % wrong" is a spot check on in-style questions
  plus hand-written must-refuse batteries, not a proof.
- The five mechanisms, the expression language, the number line (taught by counting lessons) and the thresholds are the author's
  design choices. The system selects and fits among them; it does not invent a sixth.
- **A rule that changes regime outside what it was taught (a sum that saturates, one that wraps) is confidently wrong if allowed to extrapolate.** Since 2026-10-21 the baby is *careful by default*: it states as fact only what is inside the range it was taught or has had checked; `--permissive` turns that off (see `baby/README.md`).
- **Teachers were tested only in simulation** (label noise of 2 % and 5 % of taught answers wrong, up to 200 units; independent slips up to 20 %; consistent errors about particular things). Guarding against a persistent wrong teacher costs coverage: the repair step's veto leaves it right on 74.9 / 82.0 % of the checked questions (seeds 2 / 3) against 79.2 / 88.8 % without it. No person has been the teacher at scale. Where most of one fact's own phrasings are wrong nothing can know, and at 5 % a few such answers remain;
  other noise kinds (ambiguous entities, real misspellings, noise correlated across phrasings) were not tested.
- **Real text has had only a first look (SVAMP and ASDiv word problems):** nothing is stated as a wrong fact, but the baby only guesses, and roughly 40-45 % of those labeled guesses are wrong, because wording ("how many more" against "in all") is the ceiling, not a shortage of lessons. Answers that need three numbers are not yet possible. No factoid corpus has been tried. See `baby/README.md`.
- The thresholds of the repair step (two later lessons to retire a replaced law) are my guesses, not discovered; the exam's lessons are synthetic and tidy; joules are one machine's upper bounds.
- Nothing here yet shows behavior at millions of lessons, or at "frontier" quality. Those are the open questions the
  roadmap exists to answer.

## Roadmap

1. **Scale test, continued:** clean worlds are done to 600 units (learning one lesson is flat). Next: asking about more than numeric programs, discovery at scale, and numeric learning knobs adapted from measurement, a Zipf-shaped vocabulary, discovery at scale (its pair loops are still quadratic).
2. **Compact lossless storage format**, then incremental and exact updates. (The real energy meter is done: the exam and `compare` read it.)
3. **Beyond expressions:** mechanisms of a different kind (not a formula), and the baby adapting its own learning knobs; a number
   line it discovers rather than is taught. (Building programs from a learned library, and digits as numbers, are done.)
4. **Bridge to real data:** first versions done on two word-problem corpora. Next: answers that need three numbers, a factoid corpus, and a pretrained model as the comparison (needs a rented GPU).
5. **Controlled comparison:** partly done (a matched from-scratch transformer and a tuned PyTorch one, measured joules, three seeds). Next: more sizes, the baby's energy against it on the exam, and a wrong-teacher tier of the adaptive exam.
6. **Growth:** a unit that starts tiny and adds capacity only when it saturates (function-preserving growth).

## Ambition

The long-term target is capable, continually learning AI that fits a consumer device's RAM, storage and power budget, and that
anyone can run and teach on their own hardware. Whether an architecture like this can reach that is unknown; the repository exists
to find out by measurement. The reasoning behind the project, in the author's own words, is in
[`TransientVision.md`](TransientVision.md) and [`docs/Direction.md`](docs/Direction.md).

## Documents

| file | contents |
|---|---|
| [`baby/README.md`](baby/README.md) | the experiment log: every result, dead end and bug, newest first |
| [`LEARNING.md`](LEARNING.md) | learning-efficiency research notes and sources |
| [`docs/Direction.md`](docs/Direction.md) | strategy, hardware terrain, language choice, candidate architectures |
| [`TransientVision.md`](TransientVision.md) | the author's dated vision entries |
| [`TransientHandover.md`](TransientHandover.md) | current state and next steps, for resuming work |

The website, [transientai.org](https://transientai.org), is a separate repository ([`DawsMacs/TransientAI-site`](https://github.com/DawsMacs/TransientAI-site)): overview, documentation, progress and a blog.

## License

Copyright (C) 2026 DawsMacs. Licensed under the **GNU Affero General Public License, version 3 only**
(`AGPL-3.0-only`). The aim is that this work stays open: anyone may use, study, change and share it, and anyone who
distributes it, or lets others use a modified version over a network, must offer their users the corresponding source under the
same terms. See [`LICENSE`](LICENSE); per-file licensing is declared in [`REUSE.toml`](REUSE.toml) and SPDX headers.

Contributions are welcome under the same license; see [`CONTRIBUTING.md`](CONTRIBUTING.md).
