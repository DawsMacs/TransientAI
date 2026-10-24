// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 DawsMacs
//! The controlled comparison of Vision Entry 2: the baby against a matched attention transformer, on the same lessons, graded on the same
//! questions, priced in joules (counted, and measured when the kernel lets this user read the CPU's energy counter).
//!
//! What "matched" means here, stated plainly so the comparison cannot be quietly tilted: both read the same word lists from the same training
//! lessons; both are graded on the same four question sets (taught, reworded, never taught, beyond the taught numbers); the transformer's
//! vocabulary and answer words come only from the training lessons (it can say nothing it was never taught); every size is trained until it
//! answers every training lesson, or until the epoch budget runs out, with the same optimizer and no tuning per size. The baby keeps its own
//! way of saying "I don't know"; the transformer is graded twice, always answering and answering only when its probability is at least 0.9.

use crate::data::tokenize;
use crate::kinds::{Item, World};
use crate::ledger::{fmt_joules, EnergyModel, Ledger, Rapl};
use crate::school::School;
use crate::transformer::{TConfig, Transformer};
use std::collections::HashMap;
use std::time::Instant;

/// The set names, in the order of the baby's scorecard.
pub const SETS: [&str; 4] = ["taught", "reworded", "never taught", "beyond the taught range"];

#[derive(Clone, Copy, Default, Debug)]
pub struct Grade {
    pub right: usize,
    pub refused: usize,
    pub wrong: usize,
    /// of the refused, how many had a labeled guess that was right (the baby only)
    pub guess_right: usize,
    pub n: usize,
}

impl Grade {
    fn pct(&self, x: usize) -> f64 {
        100.0 * x as f64 / self.n.max(1) as f64
    }
}

/// Real energy, if the counter is readable: joules above idle for a closure, and its wall time. The counter is the whole CPU package, so
/// whatever else the machine does is counted too. Two defenses, both learned from mistakes (see `baby/README.md`): the idle baseline is
/// **re-measured right before every measurement** (the lowest of three one-second readings; a baseline taken once at the start goes stale
/// when the background changes, and clamped later readings to 0.00 J), and every run records how much of the busy CPU time went to OTHER
/// processes, so a reading taken on a busy machine is flagged (`warning`). The energy is NOT scaled by CPU share: that was tried and it
/// under-counted even on a quiet machine.
pub struct Meter {
    rapl: Option<Rapl>,
    worst_other: std::cell::Cell<f64>,
    /// readings that came out at or below the idle baseline (the machine got quieter during the run): shown as n/a, never as 0.00 J
    invalid: std::cell::Cell<usize>,
}

/// (busy CPU seconds of the whole machine, this process's CPU seconds); a jiffy is 1/100 s.
fn cpu_seconds() -> Option<(f64, f64)> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let nums: Vec<f64> = stat.lines().next()?.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if nums.len() < 8 {
        return None;
    }
    let busy = nums.iter().sum::<f64>() - nums[3] - nums[4]; // all but idle and iowait
    let me = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = me.rsplit(')').next()?;
    let f: Vec<&str> = after.split_whitespace().collect();
    let (ut, st): (f64, f64) = (f.get(11)?.parse().ok()?, f.get(12)?.parse().ok()?);
    Some((busy / 100.0, (ut + st) / 100.0))
}

impl Meter {
    pub fn new() -> Meter {
        Meter { rapl: Rapl::open(), worst_other: std::cell::Cell::new(0.0), invalid: std::cell::Cell::new(0) }
    }

    /// A meter that measures nothing (tests: a real one samples the idle draw for seconds before and after every run).
    pub fn off() -> Meter {
        Meter { rapl: None, worst_other: std::cell::Cell::new(0.0), invalid: std::cell::Cell::new(0) }
    }

    pub fn available(&self) -> bool {
        self.rapl.is_some()
    }

    /// What the machine draws right now with nothing of ours running: the lowest of three one-second readings, in watts.
    fn idle_w(&self) -> f64 {
        let Some(r) = &self.rapl else { return 0.0 };
        let mut best = f64::MAX;
        for _ in 0..3 {
            let i0 = r.read_uj().unwrap_or(0);
            std::thread::sleep(std::time::Duration::from_secs(1));
            best = best.min(r.delta_j(i0, r.read_uj().unwrap_or(0)));
        }
        best
    }

    /// The largest share of the machine's busy CPU time that went to OTHER processes during any measurement so far (0 to 1).
    pub fn worst_other_share(&self) -> f64 {
        self.worst_other.get()
    }

    /// Run `f`; return its value, the seconds it took, and the joules above idle (None if the counter cannot be read).
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> (R, f64, Option<f64>) {
        let idle_w = self.idle_w();
        let e0 = self.rapl.as_ref().and_then(|r| r.read_uj());
        let c0 = cpu_seconds();
        let t = Instant::now();
        let out = f();
        let secs = t.elapsed().as_secs_f64();
        let e1 = self.rapl.as_ref().and_then(|r| r.read_uj());
        if let (Some((b0, m0)), Some((b1, m1))) = (c0, cpu_seconds()) {
            let busy = b1 - b0;
            if busy > 0.05 && secs > 0.1 {
                let other = ((busy - (m1 - m0)) / busy).clamp(0.0, 1.0);
                if other > self.worst_other.get() {
                    self.worst_other.set(other);
                }
            }
        }
        // the baseline is the lower of the machine's draw just before and just after the run: if the background gets quieter DURING the run
        // (a browser closed), a baseline from before alone is too high and the reading collapses to zero
        let idle_after = if self.rapl.is_some() { self.idle_w() } else { 0.0 };
        let idle = idle_w.min(idle_after);
        let above = match (&self.rapl, e0, e1) {
            (Some(r), Some(a), Some(b)) => {
                if std::env::var("METER_DEBUG").is_ok() {
                    eprintln!("METER secs {secs:.2} package {:.3} J ({:.2} W)  idle before {idle_w:.2} W after {idle_after:.2} W", r.delta_j(a, b), r.delta_j(a, b) / secs);
                }
                let net = r.delta_j(a, b) - idle * secs;
                if net > 0.0 {
                    Some(net)
                } else {
                    self.invalid.set(self.invalid.get() + 1);
                    None
                }
            }
            _ => None,
        };
        (out, secs, above)
    }

    /// A line to print at the end of an experiment if other work on the machine may have inflated the energy readings.
    pub fn warning(&self) -> Option<String> {
        let w = self.worst_other_share();
        let mut parts = Vec::new();
        if self.available() && w > 0.25 {
            parts.push(format!("other processes used up to {:.0}% of the busy CPU time during some measurements, so those energies include their share (an upper bound)", 100.0 * w));
        }
        if self.invalid.get() > 0 {
            parts.push(format!("{} energy reading(s) came out at or below the idle baseline (the machine got quieter during the run) and are shown as n/a", self.invalid.get()));
        }
        (!parts.is_empty()).then(|| format!("WARNING: {}; for clean figures rerun with the machine quiet.", parts.join("; ")))
    }
}

/// Words and answers a transformer can use: only those in the training lessons, plus spare slots reserved for what it will be taught later
/// (a network of fixed size cannot even represent a word it has no row for; the baby has no such limit, so the reserve is generous to the
/// network, not to the baby).
#[derive(Clone)]
pub struct Lexicon {
    words: HashMap<String, usize>,
    pub classes: Vec<String>,
    pub longest: usize,
    max_vocab: usize,
    max_classes: usize,
}

impl Lexicon {
    pub fn from(train: &[Item]) -> Lexicon {
        Lexicon::with_reserve(train, 0, 0)
    }

    pub fn with_reserve(train: &[Item], spare_words: usize, spare_classes: usize) -> Lexicon {
        let mut lex = Lexicon { words: HashMap::new(), classes: Vec::new(), longest: 1, max_vocab: usize::MAX, max_classes: usize::MAX };
        lex.extend(train);
        lex.max_vocab = lex.words.len() + 1 + spare_words;
        lex.max_classes = lex.classes.len() + spare_classes;
        lex
    }

    /// Give every new word and answer of these lessons a row (panics if the reserve is used up: a bug in the experiment, not a result).
    pub fn extend(&mut self, items: &[Item]) {
        for it in items {
            let toks = tokenize(&it.query);
            self.longest = self.longest.max(toks.len());
            for w in toks {
                let next = self.words.len() + 1;
                self.words.entry(w).or_insert(next);
            }
            if !self.classes.contains(&it.answer) {
                self.classes.push(it.answer.clone());
            }
        }
        assert!(self.words.len() + 1 <= self.max_vocab && self.classes.len() <= self.max_classes, "the reserve of spare words or answers is used up");
    }

    pub fn vocab(&self) -> usize {
        self.max_vocab
    }

    pub fn classes_capacity(&self) -> usize {
        self.max_classes
    }

    pub fn encode(&self, q: &str) -> Vec<usize> {
        tokenize(q).iter().map(|w| self.words.get(w).copied().unwrap_or(0)).collect()
    }

    pub fn class(&self, a: &str) -> Option<usize> {
        self.classes.iter().position(|c| c == a)
    }
}

pub fn lessons(lex: &Lexicon, items: &[Item]) -> Vec<(Vec<usize>, usize)> {
    items.iter().filter_map(|i| lex.class(&i.answer).map(|c| (lex.encode(&i.query), c))).collect()
}

/// Grade a transformer: with no threshold it always answers; with one it refuses below that probability.
pub fn grade_transformer(t: &Transformer, lex: &Lexicon, items: &[Item], threshold: f32) -> Grade {
    let mut g = Grade { n: items.len(), ..Grade::default() };
    let mut scratch = Ledger::default();
    for it in items {
        let (best, p) = t.ask(&lex.encode(&it.query), &mut scratch);
        if p < threshold {
            g.refused += 1;
        } else if lex.classes.get(best).map(|c| c.as_str()) == Some(it.answer.as_str()) {
            g.right += 1;
        } else {
            g.wrong += 1;
        }
    }
    g
}

pub fn grade_baby(school: &School, items: &[Item]) -> Grade {
    let mut g = Grade { n: items.len(), ..Grade::default() };
    for it in items {
        let r = school.ask(&it.query);
        match (&r.answer, &r.guess) {
            (Some(a), _) if *a == it.answer => g.right += 1,
            (Some(_), _) => g.wrong += 1,
            (None, guess) => {
                g.refused += 1;
                if guess.as_ref().map_or(false, |x| x.answer == it.answer) {
                    g.guess_right += 1;
                }
            }
        }
    }
    g
}

pub fn sets(w: &World) -> [&Vec<Item>; 4] {
    [&w.train, &w.reworded, &w.new_items, &w.beyond]
}

pub struct Trained {
    pub name: String,
    pub params: usize,
    pub epochs: usize,
    pub secs: f64,
    pub ledger: Ledger,
    pub measured_j: Option<f64>,
    pub model: Transformer,
}

/// Train one transformer size on the training lessons.
pub fn train_transformer(lex: &Lexicon, train: &[Item], d: usize, layers: usize, epochs: usize, seed: u64, meter: &Meter) -> Trained {
    let cfg = TConfig { vocab: lex.vocab(), classes: lex.classes_capacity(), d, heads: if d >= 64 { 4 } else { 2 }, layers, ff: 2 * d, len: lex.longest };
    let mut model = Transformer::new(cfg, seed);
    let data = lessons(lex, train);
    let mut ledger = Ledger::default();
    let (used, secs, joules) = meter.run(|| model.train(&data, epochs, 16, 0.003, seed, &mut ledger));
    Trained { name: format!("transformer d={d} x{layers}"), params: cfg.n_params(), epochs: used, secs, ledger, measured_j: joules, model }
}

fn fmt_opt_j(j: Option<f64>) -> String {
    j.map_or("n/a".to_string(), fmt_joules)
}

pub fn run(seed: u64, sizes: &[usize], layers: usize, epochs: usize) {
    let world = crate::kinds::world(seed);
    let meter = Meter::new();
    let em = EnergyModel::default();
    println!(
        "Same lessons for everyone: {} taught, graded on {} reworded, {} never taught, {} beyond the taught numbers.",
        world.train.len(),
        world.reworded.len(),
        world.new_items.len(),
        world.beyond.len()
    );
    if meter.available() {
        println!("Energy is MEASURED (CPU package counter, above idle) and also counted and priced with 45 nm figures (an estimate).");
    } else {
        println!("Energy is only counted and priced with 45 nm figures (an estimate): the CPU counter is not readable by this user.");
    }
    println!();

    // the baby
    let (school, secs, joules) = meter.run(|| School::train(&world, seed, true));
    let file_kb = {
        let tmp = crate::platform::temp_dir().join(format!("compare_{}.school", crate::platform::unique_id()));
        let path = tmp.to_string_lossy().to_string();
        let kb = school.save(&path).ok().and_then(|_| std::fs::metadata(&path).ok()).map_or(0, |m| m.len() / 1024);
        std::fs::remove_file(&path).ok();
        kb
    };
    let baby: Vec<Grade> = sets(&world).iter().map(|s| grade_baby(&school, s)).collect();

    // the transformers
    let lex = Lexicon::from(&world.train);
    let trained: Vec<Trained> = sizes.iter().map(|&d| train_transformer(&lex, &world.train, d, layers, epochs, seed, &meter)).collect();

    println!("{:<22} {:>9} {:>8} {:>7} {:>14} {:>12} {:>12}", "", "size", "epochs", "time", "counted MACs", "est. energy", "MEASURED");
    println!(
        "{:<22} {:>9} {:>8} {:>6.2}s {:>14} {:>12} {:>12}",
        "baby (school)",
        format!("{file_kb} KB file"),
        "-",
        secs,
        "(not counted)",
        "-",
        fmt_opt_j(joules)
    );
    for t in &trained {
        println!(
            "{:<22} {:>9} {:>8} {:>6.2}s {:>14} {:>12} {:>12}",
            t.name,
            format!("{} params", t.params),
            t.epochs,
            t.secs,
            t.ledger.macs,
            fmt_joules(em.joules(&t.ledger)),
            fmt_opt_j(t.measured_j)
        );
    }

    println!("\nGraded on the same questions: right / refused / WRONG (percent of the set)\n");
    print!("{:<34}", "");
    for s in SETS {
        print!(" {:>24}", s);
    }
    println!();
    let row = |name: &str, g: &[Grade]| {
        print!("{name:<34}");
        for x in g {
            print!(" {:>24}", format!("{:.1} / {:.1} / {:.1}", x.pct(x.right), x.pct(x.refused), x.pct(x.wrong)));
        }
        println!();
    };
    row("baby", &baby);
    let guess_row: Vec<String> = baby.iter().map(|g| format!("{:.1}", g.pct(g.guess_right))).collect();
    println!("{:<34} {}", "  (of its refusals, a right guess)", guess_row.iter().map(|g| format!("{:>24}", g)).collect::<Vec<_>>().join(" "));
    for t in &trained {
        let always: Vec<Grade> = sets(&world).iter().map(|s| grade_transformer(&t.model, &lex, s, 0.0)).collect();
        row(&format!("{} (always answers)", t.name), &always);
        let careful: Vec<Grade> = sets(&world).iter().map(|s| grade_transformer(&t.model, &lex, s, 0.9)).collect();
        row(&format!("{} (answers if p >= 0.9)", t.name), &careful);
    }
    if let Some(w) = meter.warning() {
        println!("\n{w}");
    }
}

fn fmt_row_grades(label: &str, new: Option<&Grade>, old: &[Grade]) {
    let cell = |g: &Grade| format!("{:.1} / {:.1} / {:.1}", g.pct(g.right), g.pct(g.refused), g.pct(g.wrong));
    print!("{label:<40}");
    match new {
        Some(g) => print!(" {:>20}", cell(g)),
        None => print!(" {:>20}", "-"),
    }
    for g in old {
        print!(" {:>20}", cell(g));
    }
    println!();
}

/// Learning something new: `n_new` invented capitals are taught (three phrasings each) and asked in a fourth. What does it cost, and what
/// happens to what was already known?
pub fn run_adapt(seed: u64, sizes: &[usize], layers: usize, epochs: usize, n_new: usize) {
    let world = crate::kinds::world(seed);
    let meter = Meter::new();
    let em = EnergyModel::default();
    let (teach_new, ask_new) = crate::kinds::new_capital_facts(seed, n_new);
    println!(
        "Learning something NEW: {} invented capitals ({} lessons, three phrasings each), asked in a fourth phrasing; then the old knowledge is re-graded.",
        n_new,
        teach_new.len()
    );
    println!("{}\n", if meter.available() { "Energy is MEASURED (CPU package, above idle) and counted (45 nm estimate)." } else { "Energy is counted only (the CPU counter is not readable)." });

    // the baby
    let mut school = School::train(&world, seed, true);
    let old_sets = [&world.train, &world.reworded, &world.new_items];
    let before: Vec<Grade> = old_sets.iter().map(|s| grade_baby(&school, s)).collect();
    let ((), secs, joules) = meter.run(|| {
        for it in &teach_new {
            let _ = school.learn(&it.query, &it.answer, Some("capital"));
        }
    });
    let after: Vec<Grade> = old_sets.iter().map(|s| grade_baby(&school, s)).collect();
    let new_grade = grade_baby(&school, &ask_new);

    println!("{:<34} {:>7} {:>9} {:>12} {:>12} {:>12}", "", "steps", "time", "counted MACs", "est. energy", "MEASURED");
    println!("{:<34} {:>7} {:>8.3}s {:>12} {:>12} {:>12}", "baby: learn the new lessons", teach_new.len(), secs, "(not counted)", "-", fmt_opt_j(joules));

    let lex0 = Lexicon::with_reserve(&world.train, 2 * n_new, n_new);
    let mut rows: Vec<(String, Grade, Vec<Grade>)> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for &d in sizes {
        let base = train_transformer(&lex0, &world.train, d, layers, epochs, seed, &meter);
        let mut lex = lex0.clone();
        lex.extend(&teach_new);
        let grade_all = |m: &Transformer| -> (Grade, Vec<Grade>) {
            (grade_transformer(m, &lex, &ask_new, 0.0), old_sets.iter().map(|s| grade_transformer(m, &lex, s, 0.0)).collect())
        };
        let (g_before, o_before) = grade_all(&base.model);
        rows.push((format!("{} before (never saw them)", base.name), g_before, o_before));
        for (label, share) in [("fine-tune on the new only", 0.0f32), ("fine-tune on new + 10% of the old", 0.1), ("fine-tune on new + all old", 1.0)] {
            let mut m = base.model.clone();
            let mut data = lessons(&lex, &teach_new);
            if share > 0.0 {
                let mut old = lessons(&lex, &world.train);
                crate::rng::Rng::new(seed ^ 0x5EED).shuffle(&mut old);
                old.truncate(((old.len() as f32) * share).round() as usize);
                data.extend(old);
            }
            let mut ledger = Ledger::default();
            let (used, secs, joules) = meter.run(|| m.train(&data, epochs, 16, 0.003, seed, &mut ledger));
            lines.push(format!(
                "{:<34} {:>7} {:>8.3}s {:>12} {:>12} {:>12}",
                format!("{} d={d}: {label}", "transformer"),
                format!("{used} ep"),
                secs,
                ledger.macs,
                fmt_joules(em.joules(&ledger)),
                fmt_opt_j(joules)
            ));
            let (gn, go) = grade_all(&m);
            rows.push((format!("transformer d={d}: {label}"), gn, go));
        }
    }
    for l in &lines {
        println!("{l}");
    }
    println!("\nright / refused / WRONG, percent of each set (transformers always answer)\n");
    print!("{:<40} {:>20}", "", "the new capitals");
    for s in ["taught", "reworded", "never taught"] {
        print!(" {:>20}", s);
    }
    println!();
    if let Some(w) = meter.warning() {
        println!("{w}\n");
    }
    fmt_row_grades("baby before", None, &before);
    fmt_row_grades("baby after learning", Some(&new_grade), &after);
    for (label, gn, go) in &rows {
        fmt_row_grades(label, Some(gn), go);
    }
}

/// How do the two grow? The same comparison on worlds of more and more units (the scale generator), trained from scratch at each size.
pub fn run_scale(seed: u64, counts: &[usize], sizes: &[usize], layers: usize, epochs: usize) {
    use crate::scale::{generate, Config};
    let meter = Meter::new();
    let em = EnergyModel::default();
    println!("{}\n", if meter.available() { "Energy is MEASURED (CPU package, above idle) and counted (45 nm estimate)." } else { "Energy is counted only." });
    println!("right / refused / WRONG, percent of each set; for the baby, in brackets, the share of its refusals that came with a right labeled guess\n");
    println!(
        "{:>6} {:>8}  {:<27} {:>8} {:>6} {:>11} {:>10}   {:>18} {:>19} {:>27}",
        "units", "lessons", "learner", "time", "epochs", "est. energy", "MEASURED", "reworded", "never taught", "beyond the taught numbers"
    );
    for &units in counts {
        let sw = generate(&Config { units, entities: (units * 20).clamp(1000, 10_000), facts_per_unit: 12, noise: 0.0, typos: 0.0, digits: false, seed });
        let w = &sw.world;
        let (school, secs, joules) = meter.run(|| School::from_items(&w.train, sw.names.clone(), Some(&w.reworded), seed, true));
        let (gr, gn, gb) = (grade_baby(&school, &w.reworded), grade_baby(&school, &w.new_items), grade_baby(&school, &w.beyond));
        let cell = |g: &Grade| format!("{:.1} / {:.1} / {:.1}", g.pct(g.right), g.pct(g.refused), g.pct(g.wrong));
        let cell_baby = |g: &Grade| format!("{} ({:.0})", cell(g), g.pct(g.guess_right) * 100.0 / g.pct(g.refused).max(0.0001));
        println!(
            "{:>6} {:>8}  {:<27} {:>7.2}s {:>6} {:>11} {:>10}   {:>18} {:>19} {:>27}",
            units, w.train.len(), "baby", secs, "-", "-", fmt_opt_j(joules), cell(&gr), cell(&gn), cell_baby(&gb)
        );
        let lex = Lexicon::from(&w.train);
        for &d in sizes {
            let t = train_transformer(&lex, &w.train, d, layers, epochs, seed, &meter);
            let g = |items: &[Item]| grade_transformer(&t.model, &lex, items, 0.0);
            println!(
                "{:>6} {:>8}  {:<27} {:>7.2}s {:>6} {:>11} {:>10}   {:>18} {:>19} {:>27}",
                "", "", format!("{} ({}k params)", t.name, t.params / 1000), t.secs, t.epochs, fmt_joules(em.joules(&t.ledger)), fmt_opt_j(t.measured_j),
                cell(&g(&w.reworded)), cell(&g(&w.new_items)), cell(&g(&w.beyond))
            );
        }
    }
    if let Some(w) = meter.warning() {
        println!("\n{w}");
    }
}

/// Write the four sets of a world as TSV (query, answer, unit, fact) so another implementation can be trained and graded on exactly the same
/// lessons and questions: `train.tsv`, `reworded.tsv`, `never.tsv`, `beyond.tsv`. With `units > 0` the world is the scale generator's.
pub fn export(seed: u64, units: usize, dir: &str) -> std::io::Result<()> {
    let world = if units > 0 {
        let c = crate::scale::Config { units, entities: (units * 20).clamp(1000, 10_000), facts_per_unit: 12, noise: 0.0, typos: 0.0, digits: false, seed };
        crate::scale::generate(&c).world
    } else {
        crate::kinds::world(seed)
    };
    std::fs::create_dir_all(dir)?;
    for (name, items) in [("train", &world.train), ("reworded", &world.reworded), ("never", &world.new_items), ("beyond", &world.beyond)] {
        let mut out = String::new();
        for it in items.iter() {
            out.push_str(&format!("{}\t{}\t{}\t{}\n", it.query, it.answer, it.unit, it.fact));
        }
        std::fs::write(format!("{dir}/{name}.tsv"), out)?;
    }
    println!("wrote {} / {} / {} / {} lessons and questions to {dir}/{{train,reworded,never,beyond}}.tsv", world.train.len(), world.reworded.len(), world.new_items.len(), world.beyond.len());
    Ok(())
}

fn refusal_reason(how: &str) -> &'static str {
    if how.contains("only taught (or have checked) numbers from") {
        "a number outside the range taught (box)"
    } else if how.contains("never together like this") {
        "numbers each inside the range but not together (corner)"
    } else if how.contains("I haven't met the word") {
        "a word it has never met"
    } else if how.contains("no kind of lesson I was taught is like") {
        "no kind of lesson is like it (routing)"
    } else if how.contains("I only memorize this kind") {
        "memorize-only unit, nothing close enough"
    } else if how.contains("couldn't work it out") {
        "the unit could not work it out (declined)"
    } else if how.contains("they disagree") {
        "rule and memory disagree"
    } else if how.contains("already shown my rule wrong") {
        "rule refuted by a correction"
    } else {
        "other"
    }
}

/// Why does the baby refuse more as the world grows? Every refusal on the never-taught questions of a scale world, tallied by the reason it
/// gave and by the kind of unit the question belongs to, at several sizes.
pub fn run_refusals(seed: u64, counts: &[usize]) {
    use crate::scale::{generate, Config};
    println!("never-taught questions the baby refused, by the reason it gave (percent of ALL never-taught questions)\n");
    for &units in counts {
        let sw = generate(&Config { units, entities: (units * 20).clamp(1000, 10_000), facts_per_unit: 12, noise: 0.0, typos: 0.0, digits: false, seed });
        let w = &sw.world;
        let school = School::from_items(&w.train, sw.names.clone(), Some(&w.reworded), seed, true);
        let n = w.new_items.len();
        let mut by_reason: Vec<(&'static str, usize)> = Vec::new();
        let mut by_kind: std::collections::BTreeMap<&str, (usize, usize)> = std::collections::BTreeMap::new();
        let (mut refused, mut guess_right, mut routed_wrong) = (0usize, 0usize, 0usize);
        let mut stretches = [0usize; 5];
        for it in &w.new_items {
            let r = school.ask(&it.query);
            let kind = sw.expect.get(it.unit).copied().unwrap_or("?");
            let e = by_kind.entry(kind).or_default();
            e.1 += 1;
            if r.answer.is_none() {
                refused += 1;
                e.0 += 1;
                if let Some(g) = &r.guess {
                    if g.answer == it.answer {
                        guess_right += 1;
                    }
                    stretches[crate::school::stretch_bucket(g.stretch)] += 1;
                }
                if r.unit.map_or(false, |u| u != it.unit) {
                    routed_wrong += 1;
                }
                let why = refusal_reason(&r.how);
                match by_reason.iter_mut().find(|(k, _)| *k == why) {
                    Some(x) => x.1 += 1,
                    None => by_reason.push((why, 1)),
                }
            }
        }
        by_reason.sort_by(|a, b| b.1.cmp(&a.1));
        println!("{units} units, {n} never-taught questions: refused {refused} ({:.1}%); of those a right labeled guess {guess_right}, sent to a different unit {routed_wrong}", 100.0 * refused as f64 / n as f64);
        for (why, c) in &by_reason {
            println!("    {:>5.1}%  {why}", 100.0 * *c as f64 / n as f64);
        }
        println!("    how far past the taught range the refused questions are (stretch, in widths of the range): under 0.25: {}, to 0.5: {}, to 1: {}, to 2: {}, over 2: {}", stretches[0], stretches[1], stretches[2], stretches[3], stretches[4]);
        for (kind, (r, t)) in &by_kind {
            println!("    units that should be '{kind}': {r} of {t} refused ({:.1}%)", 100.0 * *r as f64 / (*t).max(1) as f64);
        }
        println!();
    }
}

/// Teacher checks, automatic. The baby asks its own check questions (`check_myself`: the edges and corner of what each rule was taught, a few
/// per unit per round); a simulated teacher who knows each unit's operation answers the ones that have an answer; the baby learns the answers.
/// How many questions does it take to clear the refusals, and does anything it states as a fact ever turn out wrong?
pub fn run_teacher(seed: u64, units: usize, rounds: usize, per_unit: usize, wrong_rate: f32, skeptical: bool, persistent: bool) {
    use crate::scale::{generate, teacher_truth, Config};
    let sw = generate(&Config { units, entities: (units * 20).clamp(1000, 10_000), facts_per_unit: 12, noise: 0.0, typos: 0.0, digits: false, seed });
    let w = &sw.world;
    let mut school = School::from_items(&w.train, sw.names.clone(), Some(&w.reworded), seed, true);
    school.set_skeptical(skeptical);
    let mut teacher_rng = crate::rng::Rng::new(seed ^ 0x7EAC4E5);
    // a teacher who is sometimes wrong: with this probability the answer is some other number word
    // persistent: consistently wrong about particular questions (the same wrong answer every time it is asked), which a second opinion cannot catch
    let mut say = |truth: usize, rng: &mut crate::rng::Rng, question: &str| -> (usize, bool) {
        let mut r = if persistent {
            let h = question.bytes().fold(0xcbf29ce484222325u64 ^ seed, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
            crate::rng::Rng::new(h)
        } else {
            crate::rng::Rng::new(rng.next_u64())
        };
        if wrong_rate > 0.0 && r.uniform() < wrong_rate {
            let mut other = r.below(11);
            if other == truth {
                other = (other + 1) % 11;
            }
            (other, true)
        } else {
            (truth, false)
        }
    };
    let (mut wrong_given, mut wrong_learned, mut second_opinions) = (0usize, 0usize, 0usize);
    let mut poisoned: std::collections::HashSet<String> = std::collections::HashSet::new();
    // the same facts regardless of phrasing: (unit, the numbers in the order they appear)
    let mut poisoned_facts: std::collections::HashSet<(String, Vec<usize>)> = std::collections::HashSet::new();
    let numbers_of = |q: &str| -> Vec<usize> { crate::data::tokenize(q).iter().filter_map(|t| crate::data::NUM.iter().position(|n| n == t)).collect() };
    let numeric = sw.sems.iter().filter(|s| s.is_some()).count();
    println!("{units} units ({numeric} numeric), {} lessons; per round the baby asks up to {per_unit} check questions per unit, a simulated teacher answers those the number words can express\n", w.train.len());
    println!(
        "{:<22} {:>9} {:>9}   {:>26} {:>26}   {:>12}",
        "", "asked", "answered", "never taught r / ref / WRONG", "beyond range r / ref / WRONG", "refused had a right guess"
    );
    let report = |school: &School, label: &str, asked: usize, answered: usize| {
        let (gn, gb) = (grade_baby(school, &w.new_items), grade_baby(school, &w.beyond));
        let cell = |g: &Grade| format!("{:.1} / {:.1} / {:.1}", g.pct(g.right), g.pct(g.refused), g.pct(g.wrong));
        println!("{label:<22} {asked:>9} {answered:>9}   {:>26} {:>26}   {:>12}", cell(&gn), cell(&gb), format!("{:.0}%", 100.0 * gn.guess_right as f64 / gn.refused.max(1) as f64));
        gn.wrong + gb.wrong
    };
    let floor_start = school.route_floor;
    let mut wrong_ever = report(&school, "before any checks", 0, 0);
    // how well each never-taught question was recognized before any learning: the margin of its best unit over the runner-up, and who won
    let before_route: Vec<(usize, f32)> = w.new_items.iter().map(|it| school.route(&it.query, &mut Ledger::default())).collect();
    let floor_before = school.route_floor;
    let (mut asked, mut answered, mut unanswerable) = (0usize, 0usize, 0usize);
    for round in 1..=rounds {
        let checks = school.check_myself(per_unit);
        let mut learned_this_round = 0usize;
        for c in &checks {
            asked += 1;
            let Some(u) = sw.names.iter().position(|n| *n == c.unit) else { continue };
            let Some(sem) = sw.sems[u] else { continue };
            let nums: Vec<usize> = crate::data::tokenize(&c.question).iter().filter_map(|t| crate::data::NUM.iter().position(|n| n == t)).collect();
            match teacher_truth(sem, &nums, false) {
                Some(a) => {
                    // the teacher answers; if the baby doubts the answer (skeptical mode) it asks the same question again, up to twice more
                    let mut last = None;
                    for attempt in 0..3 {
                        let (given, wrong) = say(a, &mut teacher_rng, &c.question);
                        if attempt > 0 {
                            asked += 1;
                            second_opinions += 1;
                        }
                        wrong_given += wrong as usize;
                        match school.learn_wondered(&c.question, crate::data::NUM[given], &c.unit) {
                            Ok(l) if l.disputed => last = Some(false),
                            Ok(_) => {
                                if wrong {
                                    wrong_learned += 1;
                                    poisoned.insert(c.question.clone());
                                    poisoned_facts.insert((c.unit.clone(), numbers_of(&c.question)));
                                }
                                last = Some(true);
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                    if last == Some(true) {
                        answered += 1;
                        learned_this_round += 1;
                    }
                }
                None => unanswerable += 1,
            }
        }
        if std::env::var("UNIT_DEBUG").is_ok() {
            let mut shown = 0;
            for (u, sem) in sw.sems.iter().enumerate() {
                if matches!(sem, Some(crate::scale::Sem::Apart)) && shown < 2 {
                    shown += 1;
                    println!("  [round {round}] {}  program {:?}", school.describe_unit_open(u), school.units[u].program());
                    if shown == 1 {
                        if let Some(it) = w.beyond.iter().filter(|i| i.unit == u).find(|i| school.ask(&i.query).answer.is_none()) {
                            println!("      refused beyond: {:?} -> {}", it.query, school.ask(&it.query).how.chars().take(120).collect::<String>());
                            println!("      {}", school.debug_reach(u, &it.query));
                        }
                    }
                }
            }
        }
        if std::env::var("RECAL_EACH_ROUND").is_ok() {
            school.recalibrate_route();
        }
        wrong_ever += report(&school, &format!("after round {round}"), asked, answered);
        if checks.is_empty() || learned_this_round == 0 {
            println!("  (nothing left to ask or answer after round {round})");
            break;
        }
    }
    if let Ok(f) = std::env::var("FLOOR_SCALE") {
        // (experiment only) what if the recognition floor had moved with the margins?
        school.route_floor *= f.parse::<f32>().unwrap_or(1.0);
        let (gn, gb) = (grade_baby(&school, &w.new_items), grade_baby(&school, &w.beyond));
        println!("with the recognition floor scaled by {f} (to {:.3}): never taught {:.1} / {:.1} / {:.1}, beyond {:.1} / {:.1} / {:.1} (right / refused / WRONG)", school.route_floor, gn.pct(gn.right), gn.pct(gn.refused), gn.pct(gn.wrong), gb.pct(gb.right), gb.pct(gb.refused), gb.pct(gb.wrong));
        let traps = sw.must_refuse.iter().filter(|(q, _)| school.ask(q).answer.is_some()).count();
        println!("must-refuse traps answered: {traps} of {}", sw.must_refuse.len());
    }
    // what is left: every refused never-taught question, by the reason and by what the guess (if any) was
    let (mut left, mut none_guess, mut right_guess, mut wrong_guess) = (0usize, 0usize, 0usize, 0usize);
    let mut reasons: Vec<(&'static str, usize)> = Vec::new();
    for it in &w.new_items {
        let r = school.ask(&it.query);
        if r.answer.is_none() {
            left += 1;
            match &r.guess {
                None => none_guess += 1,
                Some(g) if g.answer == it.answer => right_guess += 1,
                Some(_) => wrong_guess += 1,
            }
            let why = refusal_reason(&r.how);
            match reasons.iter_mut().find(|(k, _)| *k == why) {
                Some(x) => x.1 += 1,
                None => reasons.push((why, 1)),
            }
        }
    }
    reasons.sort_by(|a, b| b.1.cmp(&a.1));
    if std::env::var("ROUTE_DEBUG").is_ok() {
        println!("\nroute floor before {floor_before:.3}, after {:.3}", school.route_floor);
        let mut shown = 0;
        for (i, it) in w.new_items.iter().enumerate() {
            let r = school.ask(&it.query);
            if r.answer.is_none() && refusal_reason(&r.how) == "no kind of lesson is like it (routing)" && shown < 8 {
                shown += 1;
                let (u_now, m_now) = school.route(&it.query, &mut Ledger::default());
                println!(
                    "  {:?}: true unit {} ({} lessons now), margin before {:.2} (best unit {}), now {:.2} (best unit {}, {} lessons now)",
                    it.query, it.unit, school.units[it.unit].facts(), before_route[i].1, before_route[i].0, m_now, u_now, school.units[u_now].facts()
                );
            }
        }
    }
    println!("\nstill refused among the never-taught: {left} (a right guess {right_guess}, a WRONG guess {wrong_guess}, no guess {none_guess})");
    for (why, c) in &reasons {
        println!("    {c:>5}  {why}");
    }
    if std::env::var("UNIT_DEBUG").is_ok() {
        for (u, sem) in sw.sems.iter().enumerate() {
            if matches!(sem, Some(crate::scale::Sem::Apart)) {
                let items: Vec<&Item> = w.beyond.iter().filter(|i| i.unit == u).collect();
                let refused = items.iter().filter(|i| school.ask(&i.query).answer.is_none()).count();
                println!("  Apart unit {u}: {refused} of {} beyond refused; {}", items.len(), school.describe_unit_open(u));
            }
        }
    }
    // the beyond-range questions that are still refused: why, and which operations
    {
        let mut by_reason: Vec<(&'static str, usize)> = Vec::new();
        let mut by_sem: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
        let mut shown = 0;
        let total = w.beyond.len();
        let mut left = 0usize;
        for it in &w.beyond {
            let r = school.ask(&it.query);
            let sem = sw.sems.get(it.unit).copied().flatten().map_or("facts".to_string(), |s| format!("{s:?}"));
            let e = by_sem.entry(sem.clone()).or_default();
            e.1 += 1;
            if r.answer.is_none() {
                left += 1;
                e.0 += 1;
                let why = refusal_reason(&r.how);
                match by_reason.iter_mut().find(|(k, _)| *k == why) {
                    Some(x) => x.1 += 1,
                    None => by_reason.push((why, 1)),
                }
                if shown < 10 && std::env::var("BEYOND_DEBUG").is_ok() {
                    shown += 1;
                    println!("    e.g. {:?} ({sem}) -> {}", it.query, r.how.chars().take(150).collect::<String>());
                }
            }
        }
        by_reason.sort_by(|a, b| b.1.cmp(&a.1));
        println!("\nstill refused among the {total} beyond-range questions: {left}");
        for (why, c) in &by_reason {
            println!("    {c:>5}  {why}");
        }
        for (sem, (r, t)) in &by_sem {
            println!("    {sem:<8} {r:>4} of {t:>4} refused");
        }
    }
    println!("\nrecognition floor: {floor_start:.3} when taught, {:.3} after all the learning", school.route_floor);
    if std::env::var("ROUTE_DEBUG").is_ok() {
        let m = school.loo_margins();
        let at = |q: f32| m[((m.len() as f32 * q) as usize).min(m.len() - 1)];
        println!("leave-one-out margins of its own lessons ({} sampled): min {:.2}, 1% {:.2}, 2% {:.2}, 5% {:.2}, 10% {:.2}, median {:.2}", m.len(), m[0], at(0.01), at(0.02), at(0.05), at(0.10), at(0.5));
        let mut unseen: Vec<f32> = w.new_items.iter().map(|it| school.route(&it.query, &mut Ledger::default()).1).collect();
        unseen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let u = |q: f32| unseen[((unseen.len() as f32 * q) as usize).min(unseen.len() - 1)];
        println!("margins of the never-taught questions ({}): min {:.2}, 1% {:.2}, 2% {:.2}, 5% {:.2}, 10% {:.2}, median {:.2}", unseen.len(), unseen[0], u(0.01), u(0.02), u(0.05), u(0.10), u(0.5));
    }
    if wrong_rate > 0.0 {
        // wrong facts: a question it was wrongly told and now repeats, against a question it was never wrongly told
        let (mut repeats, mut elsewhere, mut refused, mut right, mut total) = (0usize, 0usize, 0usize, 0usize, 0usize);
        for it in w.new_items.iter().chain(&w.beyond) {
            total += 1;
            let r = school.ask(&it.query);
            match r.answer {
                Some(a) if a == it.answer => right += 1,
                Some(_) if poisoned_facts.contains(&(sw.names[it.unit].clone(), numbers_of(&it.query))) => repeats += 1,
                Some(_) => elsewhere += 1,
                None => refused += 1,
            }
        }
        println!(
            "\nthe teacher was wrong {wrong_given} times (learned {wrong_learned}, {second_opinions} second opinions asked, {} answers still held); {} of {} units refuted. Of {total} unseen and beyond-range questions: right {:.2}%, refused {:.2}%, WRONG {} (of which {repeats} repeat a fact it was wrongly told, in any phrasing, and {elsewhere} are damage elsewhere)",
            school.disputed_count(),
            school.refuted_units(),
            school.units.len(),
            100.0 * right as f64 / total as f64,
            100.0 * refused as f64 / total as f64,
            repeats + elsewhere
        );
    }
    if wrong_rate > 0.0 && std::env::var("WRONG_DEBUG").is_ok() {
        let mut shown = 0;
        for it in w.new_items.iter().chain(&w.beyond) {
            let r = school.ask(&it.query);
            if let Some(a) = &r.answer {
                if *a != it.answer && !poisoned_facts.contains(&(sw.names[it.unit].clone(), numbers_of(&it.query))) && shown < 6 {
                    shown += 1;
                    let sem = sw.sems.get(it.unit).copied().flatten();
                    let told: Vec<&String> = poisoned.iter().filter(|q| q.contains(sw.names[it.unit].split('-').nth(1).unwrap_or("?"))).collect();
                    println!("  WRONG {:?} ({sem:?}): truth {}, said {a}: {}", it.query, it.answer, r.how.chars().take(110).collect::<String>());
                    println!("      unit: {}  |  wrongly told in this unit: {:?}", school.describe_unit_open(it.unit), told);
                    println!("      {}", school.debug_reach(it.unit, &it.query));
                }
            }
        }
    }
    if wrong_rate > 0.0 && std::env::var("POISON_DEBUG").is_ok() {
        let mut by_unit: std::collections::BTreeMap<usize, Vec<&String>> = std::collections::BTreeMap::new();
        for q in &poisoned {
            if let Some(u) = (0..school.units.len()).find(|&u| q.contains(sw.names[u].split('-').nth(1).unwrap_or("?"))) {
                by_unit.entry(u).or_default().push(q);
            }
        }
        for (u, qs) in by_unit.iter().take(8) {
            let refused = w.beyond.iter().chain(&w.new_items).filter(|i| i.unit == *u).find(|i| school.ask(&i.query).answer.is_none());
            println!("  unit {u} ({:?}), told wrongly {:?}: {}", sw.sems[*u], qs, school.describe_unit_open(*u));
            if let Some(it) = refused {
                println!("      e.g. refused {:?}: {}", it.query, school.ask(&it.query).how.chars().take(130).collect::<String>());
                println!("      {}", school.debug_reach(*u, &it.query));
            }
        }
    }
    // the traps: questions that must never be answered (a negative result, a result with no number word, two attributes at once, ...)
    let traps = sw.must_refuse.iter().filter(|(q, _)| school.ask(q).answer.is_some()).count();
    if std::env::var("TRAP_DEBUG").is_ok() {
        for (q, why) in sw.must_refuse.iter().filter(|(q, _)| school.ask(q).answer.is_some()) {
            let r = school.ask(q);
            eprintln!("TRAP ANSWERED {q:?} ({why}) -> {:?}: {}", r.answer, r.how);
        }
    }
    println!("\nmust-refuse traps answered after all that learning: {traps} of {}", sw.must_refuse.len());
    println!("\n{unanswerable} check questions had no answer a teacher could give in number words (a result past ten, a negative result); wrong answers stated as fact, summed over every report: {wrong_ever}");
}
