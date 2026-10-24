// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 DawsMacs
//! One baby. It is taught a curriculum of *units* of lessons; for each unit it works out, from evidence, how to learn it (see
//! `mechanisms.rs`); a question is sent to the unit it resembles, answered with that unit's mechanism, and refused when the
//! baby has no business answering: a kind of question it was never taught, a unit where it could only ever memorize, a word it
//! has never met, or something its mechanism does not understand. Every answer, and every refusal, says where it came from.
//!
//! It keeps every lesson it was ever taught, so it can go on learning after school: a new lesson joins a unit (or opens one),
//! the unit's way of learning is worked out again from all its lessons, and the baby carries on.
//!
//! The work of answering is counted in a `Ledger`. Two choices keep it small: questions are sent to a unit by a precomputed table
//! of how telling each word is for each unit (a few lookups) instead of comparing the question with every lesson ever taught, and
//! remembered questions are found through an index of their content words instead of scanning them all.

use crate::data::tokenize;
use crate::kinds::{Item, World, UNITS};
use crate::ledger::Ledger;
use crate::mechanisms::{self, choose_for_unit, choose_mechanism, evidence_thrifty, fit, predict, shape, Fitted, Mech, Remembered, SelectRule, Weights};
use crate::model::Model;
use crate::net::Scratch;
use crate::program::{Expr, Library, ProgramRule};
use crate::quantity::{counting_lessons, NumberLine, Quantity};
use std::collections::HashMap;
use std::fmt::Write;

pub struct SchoolUnit {
    pub name: String,
    pub chosen: Mech,
    pub evidence: [f64; 5],
    /// Everything it was ever taught in this unit.
    lessons: Vec<Item>,
    remember: Remembered,
    state: Fitted,
    /// Indices of lessons the current rule, selection or program does not explain (sorted). Where a rule can be trusted is read off the lessons it
    /// DOES explain; a lesson it gets wrong does not widen that.
    exceptions: Vec<usize>,
    /// An unexplained lesson lies outside what the explained lessons span: the rule has been shown wrong out there (a teacher's correction of a
    /// guess, say), so it may not even guess beyond its explained range.
    refuted: bool,
    /// For a rule or a selection: the total weight of each lesson's non-number words, i.e. the FUNCTION that lesson asks for (a rule predicts with
    /// the sum of its words' weight vectors, so every phrasing of one operation has about the same total). Empty for other mechanisms.
    signatures: Vec<[f32; 3]>,
}

impl SchoolUnit {
    /// (for diagnosis) what a rule or selection unit has learned about a word
    pub fn describe_word(&self, w: &str) -> String {
        let n = self.lessons.iter().filter(|l| l.query.contains(w)).count();
        match &self.state {
            Fitted::Rule(q) => format!("rule: {:?}; {n} lessons mention it of {}", q.operator(w), self.lessons.len()),
            Fitted::Select(_) => format!("select; {n} lessons mention it of {}", self.lessons.len()),
            _ => format!("{}; {n} lessons mention it of {}", self.chosen.name(), self.lessons.len()),
        }
    }

    /// (for diagnosis) the competing explanations a program unit holds, and the ambiguity the lessons show now
    pub fn describe_open(&self, lib: &Library) -> String {
        let alts = match &self.state {
            Fitted::Program(Some(p)) => p.alts.iter().map(|e| e.show()).collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let amb = ProgramRule::ambiguity(&self.lessons, lib).map(|a| (a.hypotheses.iter().map(|e| e.show()).collect::<Vec<_>>(), a.inputs));
        format!("chose {} evidence {:?} facts {} alts {alts:?} ambiguity {amb:?} lib consts {:?} funcs {}", self.chosen.name(), self.evidence.map(|e| (e * 100.0).round()), self.facts(), lib.consts, lib.funcs.len())
    }

    /// Do this unit's lessons give their answers in words (not digits)? (for a person typing "6" where the unit says "six")
    pub fn speaks_in_words(&self) -> bool {
        !self.lessons.is_empty() && self.lessons.iter().take(8).all(|l| l.answer.parse::<f64>().is_err())
    }

    pub fn best_evidence(&self) -> f64 {
        self.evidence.iter().cloned().fold(0.0, f64::max)
    }

    /// True when nothing it tried generalized: the baby can only repeat what it was told in this unit.
    pub fn memorize_only(&self) -> bool {
        self.best_evidence() < 0.8
    }

    pub fn facts(&self) -> usize {
        self.lessons.iter().map(|l| l.fact).collect::<std::collections::BTreeSet<_>>().len()
    }

    pub fn lessons(&self) -> usize {
        self.lessons.len()
    }

    /// The expression it built, if its mechanism is a program.
    pub fn program(&self) -> Option<String> {
        match &self.state {
            Fitted::Program(Some(p)) => Some(p.expr.show()),
            _ => None,
        }
    }

    /// (lessons the program was repaired around, the way of learning it replaced): for tracing
    pub fn repair_info(&self) -> (usize, Option<String>) {
        match &self.state {
            Fitted::Program(Some(p)) => (p.repaired.len(), p.before.clone()),
            _ => (0, None),
        }
    }
}

/// How telling each word's *shape* is for each unit. A question's score for a unit is the sum, over its words, of
/// ln(P(word | unit) / P(word)); sending it takes a lookup and a few additions per word.
///
/// Kept as counts, sparsely: for each word only the units it occurs in, so the table grows with what was taught, not with
/// vocabulary x units, and one more lesson touches only its own words (`add_lesson`). Scores drop two terms that are the same for every
/// unit (they cancel in the margin between the best unit and the runner-up, which is all that is ever used):
///   score(u) = sum over known words t of ln(1 + 2 * count(t, u))  -  (number of known words) * ln(lessons(u) + 1 + V / 100),
/// with V the number of distinct words: additive smoothing, (count + 1/2) / (lessons + 1 + V / 100). With a small constant instead of V / 2 a tiny
/// unit that never saw a rare word was penalized far less for that than a big unit that had seen it rarely, so a 15-lesson unit won
/// questions about words that only the big merged unit knew. (V / 2 is the textbook choice and V / 20 both hurt: the first a small clean world, the second 600 units, where they blur the units apart; V / 100 keeps both exact, before and after merging.)
#[derive(PartialEq, Debug)]
struct Routing {
    rows: HashMap<String, Vec<Cell>>,
    lessons: Vec<u32>,
    cells: usize,
    /// Error-driven corrections to the scores: per word, one number per unit, learned by `refine` from the table's own mistakes on its lessons
    /// (empty unless routing by counting is measurably poor, see `settle_route_precision`). Counting alone treats every word as independent
    /// evidence and cannot learn that a word matters only against a particular rival; a correction learned from mistakes can.
    delta: HashMap<String, Vec<f32>>,
    /// When set, the corrections ARE the score (learned from zero by `refine`); the counts are not added.
    pure: bool,
}

#[derive(PartialEq, Debug, Clone, Copy)]
struct Cell {
    unit: u32,
    /// How many of the unit's lessons contain the word.
    count: u32,
    /// ln(1 + 2 * count)
    gain: f32,
}

impl Routing {
    fn build(units: &[SchoolUnit], line: &NumberLine) -> Routing {
        let mut r = Routing { rows: HashMap::new(), lessons: Vec::new(), cells: 0, delta: HashMap::new(), pure: false };
        for (u, unit) in units.iter().enumerate() {
            r.ensure_units(u + 1);
            for l in &unit.lessons {
                r.add_lesson(u, &shape(&l.query, line));
            }
        }
        r
    }

    /// As `build`, leaving out every fifth lesson from position `fold` (counted across all units): to see how well it routes what it was not shown.
    fn build_without(units: &[SchoolUnit], line: &NumberLine, fold: usize) -> Routing {
        let mut r = Routing { rows: HashMap::new(), lessons: Vec::new(), cells: 0, delta: HashMap::new(), pure: false };
        let mut position = 0usize;
        for (u, unit) in units.iter().enumerate() {
            r.ensure_units(u + 1);
            for l in &unit.lessons {
                if position % ROUTE_FOLDS != fold {
                    r.add_lesson(u, &shape(&l.query, line));
                }
                position += 1;
            }
        }
        r
    }

    /// Learn the corrections (`delta`) from the table's mistakes: each sample is (its unit, the shape of its question), scored the way an unseen question
    /// of that unit would be (itself taken out of the table), and whenever the right unit does not win by a margin the words of the question are moved
    /// toward the right unit and away from the winner (a perceptron on top of the counts). The corrections are averaged over the epochs.
    fn refine(&mut self, samples: &[(usize, Vec<String>)], epochs: usize) {
        self.delta.clear();
        let units = self.lessons.len();
        if units < 2 || samples.is_empty() {
            return;
        }
        let base: Vec<Vec<f32>> = if self.pure { vec![vec![0.0; units]; samples.len()] } else { samples.iter().map(|(u, sh)| self.scores_without(sh, *u)).collect() };
        let mut w: HashMap<String, Vec<f32>> = HashMap::new();
        let mut total: HashMap<String, Vec<f32>> = HashMap::new();
        let mut order: Vec<usize> = (0..samples.len()).collect();
        let mut rng = crate::rng::Rng::new(0x5EED ^ samples.len() as u64);
        for _ in 0..epochs {
            // (in unit order a perceptron forgets all but the last unit it saw)
            rng.shuffle(&mut order);
            for &i in &order {
                let (u, sh) = &samples[i];
                let mut s = base[i].clone();
                for t in sh {
                    if let Some(v) = w.get(t) {
                        for (x, c) in s.iter_mut().zip(v) {
                            *x += c;
                        }
                    }
                }
                let (mut rival, mut best) = (usize::MAX, f32::NEG_INFINITY);
                for (k, x) in s.iter().enumerate() {
                    if k != *u && *x > best {
                        best = *x;
                        rival = k;
                    }
                }
                if rival != usize::MAX && s[*u] - best < 1.0 {
                    for t in sh {
                        let v = w.entry(t.clone()).or_insert_with(|| vec![0.0; units]);
                        v[*u] += 1.0;
                        v[rival] -= 1.0;
                    }
                }
            }
            for (t, v) in &w {
                let a = total.entry(t.clone()).or_insert_with(|| vec![0.0; units]);
                for (x, c) in a.iter_mut().zip(v) {
                    *x += c;
                }
            }
        }
        for (t, mut v) in total {
            for x in v.iter_mut() {
                *x /= epochs as f32;
            }
            self.delta.insert(t, v);
        }
    }

    fn add_delta(&self, shape: &[String], s: &mut [f32]) {
        for t in shape {
            if let Some(d) = self.delta.get(t) {
                for (x, c) in s.iter_mut().zip(d) {
                    *x += c;
                }
            }
        }
    }

    fn ensure_units(&mut self, n: usize) {
        while self.lessons.len() < n {
            self.lessons.push(0);
        }
    }

    /// One more lesson in unit `u`, given the shape of its question.
    fn add_lesson(&mut self, u: usize, shape: &[String]) {
        self.ensure_units(u + 1);
        self.lessons[u] += 1;
        for t in shape {
            let row = self.rows.entry(t.clone()).or_default();
            match row.binary_search_by_key(&(u as u32), |c| c.unit) {
                Ok(i) => {
                    row[i].count += 1;
                    row[i].gain = (1.0 + 2.0 * row[i].count as f32).ln();
                }
                Err(i) => {
                    row.insert(i, Cell { unit: u as u32, count: 1, gain: 3.0f32.ln() });
                    self.cells += 1;
                }
            }
        }
    }

    /// The scores `shape` would get if one lesson of unit `unit` with exactly this shape had never been added to the table: what an unseen
    /// question of a known unit looks like to the table. (Mirrors `add_lesson` and `scores`: counts go down by the number of occurrences of
    /// each word, a word nobody else has used becomes unknown, and the unit has one lesson fewer.)
    fn scores_without(&self, shape: &[String], unit: usize) -> Vec<f32> {
        let units = self.lessons.len();
        let mut s = vec![0.0f32; units];
        if self.pure {
            self.add_delta(shape, &mut s);
            return s;
        }
        let mut known = 0usize;
        let mut occ: HashMap<&str, u32> = HashMap::new();
        for t in shape {
            *occ.entry(t.as_str()).or_default() += 1;
        }
        for t in shape {
            if let Some(row) = self.rows.get(t) {
                let o = occ[t.as_str()];
                let mut left = false;
                for c in row {
                    let count = if c.unit as usize == unit { c.count.saturating_sub(o) } else { c.count };
                    if count > 0 {
                        left = true;
                        s[c.unit as usize] += (1.0 + 2.0 * count as f32).ln();
                    }
                }
                if left {
                    known += 1;
                }
            }
        }
        for t in shape {
            if let Some(d) = self.delta.get(t) {
                for (x, c) in s.iter_mut().zip(d) {
                    *x += c;
                }
            }
        }
        let half_vocabulary = 1.0 + self.rows.len() as f32 / 100.0;
        for (u, x) in s.iter_mut().enumerate() {
            let n = if u == unit { self.lessons[u].saturating_sub(1) } else { self.lessons[u] };
            *x -= known as f32 * (n as f32 + half_vocabulary).ln();
        }
        s
    }

    fn scores(&self, shape: &[String], ledger: &mut Ledger) -> Vec<f32> {
        let units = self.lessons.len();
        let mut s = vec![0.0f32; units];
        if self.pure {
            self.add_delta(shape, &mut s);
            ledger.words_read += (shape.len() + units) as u64;
            ledger.macs += (shape.len() * units) as u64;
            return s;
        }
        let (mut known, mut touched) = (0usize, 0usize);
        for t in shape {
            if let Some(row) = self.rows.get(t) {
                known += 1;
                touched += row.len();
                for c in row {
                    s[c.unit as usize] += c.gain;
                }
            }
        }
        for t in shape {
            if let Some(d) = self.delta.get(t) {
                for (x, c) in s.iter_mut().zip(d) {
                    *x += c;
                }
                touched += d.len();
            }
        }
        let half_vocabulary = 1.0 + self.rows.len() as f32 / 100.0;
        for (x, &n) in s.iter_mut().zip(&self.lessons) {
            *x -= known as f32 * (n as f32 + half_vocabulary).ln();
        }
        ledger.words_read += (shape.len() + touched + units) as u64;
        ledger.macs += (touched + units) as u64;
        ledger.touch(self.cells * std::mem::size_of::<Cell>());
        s
    }
}

/// The features the learned router sees: the shape of the question (a bag of its words) plus its last `ROUTE_TAIL` words marked as such. A story
/// problem asks its question last, and what it asks decides what to do with the numbers; the bag alone cannot say which words were the question.
fn routing_features(q: &str, line: &NumberLine) -> Vec<String> {
    let mut out = shape(q, line);
    let words = tokenize(q);
    for w in words.iter().rev().take(ROUTE_TAIL) {
        out.push(if line.value(w).is_some() { "tail:<num>".to_string() } else { format!("tail:{w}") });
    }
    out
}

const ROUTE_TAIL: usize = 8;
static PROF: std::sync::Mutex<Vec<(&'static str, f64, u64)>> = std::sync::Mutex::new(Vec::new());

/// Run `f`, and when BABY_PROF is set add its wall time to the stage called `name` (for finding where the energy of learning goes).
fn timed<R>(name: &'static str, f: impl FnOnce() -> R) -> R {
    if std::env::var("BABY_PROF").is_err() {
        return f();
    }
    let t = std::time::Instant::now();
    let r = f();
    let dt = t.elapsed().as_secs_f64();
    let mut p = PROF.lock().unwrap();
    match p.iter_mut().find(|x| x.0 == name) {
        Some(x) => {
            x.1 += dt;
            x.2 += 1;
        }
        None => p.push((name, dt, 1)),
    }
    r
}

/// (for diagnosis) the stages timed so far: name, seconds, calls.
pub fn print_prof() {
    let p = PROF.lock().unwrap();
    let total: f64 = p.iter().map(|x| x.1).sum();
    for (name, secs, calls) in p.iter() {
        eprintln!("PROF {name:<28} {secs:>8.2} s  {calls:>6} calls  {:>6.1} ms each", 1000.0 * secs / *calls as f64);
    }
    eprintln!("PROF (stages overlap: a stage may contain others) total listed {total:.1} s");
}

const ROUTE_FOLDS: usize = 5;
/// A repaired program overrules the law it replaced once this many lessons told after the repair have separated the two.
const RETIRE_AFTER: usize = 2;

/// What to do when a fresh fit offers a different exact program for a unit that already has an established one (`School::rebuild_policy`).
enum Rebuild {
    Ordinary,
    /// adopt the fresh program as a repair around these lessons (their queries)
    Adopt(Vec<String>),
}
/// A settled exact unit takes in this many told lessons between thinking its way of learning through again from scratch.
const SETTLED_RETHINK_EVERY: usize = 25;
/// How many lessons a unit that only guesses takes in between looking again at its program.
const GUESSER_REFIT_EVERY: usize = 25;
/// After this many lessons it was told, the routing corrections are learned again.
const ROUTE_RETRAIN_EVERY: usize = 20;
/// Passes over its own lessons when learning the routing corrections (`Routing::refine`).
const ROUTE_EPOCHS: usize = 12;

/// Units that follow one rule together, and the held-back evidence that says so (mechanism index into `Mech::ALL`, accuracy).
struct Group {
    units: Vec<usize>,
    fit: Option<(usize, f64)>,
}

pub struct School {
    pub units: Vec<SchoolUnit>,
    weights: Weights,
    routing: Routing,
    line: NumberLine,
    /// How clearly one unit must beat the next for a question to count as a recognizable kind: learned from rewordings.
    pub route_floor: f32,
    /// The floor as it was when the baby was taught, the ratio of that floor to the leave-one-out margin measured then (0: unknown), and how many
    /// lessons there were when the floor was last set. See `recalibrate_route`.
    route_base: f32,
    route_ratio: f32,
    calibrated_lessons: usize,
    pub seed: u64,
    /// The library as it was when the units were last fitted or reconsidered.
    library_seen: String,
    /// The reliability bar measured from the noise in its lessons (see `mechanisms::bar_for_noise`).
    bar: f64,
    /// Careful about extrapolating: a rule, selection or program answers only for numbers within the range it was taught or has had checked
    /// (see `beyond_reach`). On by default (Daws, 2026-10-21): a rule that is right on what it was taught usually is right beyond it, but one
    /// that changes regime outside it (a sum that saturates, one that wraps) is then confidently wrong, and nothing in the lessons can say so.
    /// `set_careful(false)` is the permissive showcase.
    careful: bool,
    /// Skeptical of a teacher whose answer contradicts a settled rule: hold it, ask again, believe it only when it is repeated. On by default
    /// (Daws, 2026-10-22, on the baby's recommendation: against independent slips it keeps accuracy at 99.9-100 % up to 20 % wrong answers).
    skeptical: bool,
    /// Answers held back for a second opinion: (unit, question, the answer that contradicted its rule).
    disputed: Vec<(String, String, String)>,
    /// Its own record as a guesser: for each distance bucket, (guesses that turned out right, wrong). Learned from the answers it is given.
    guess_record: [(u32, u32); 5],
    /// Units whose repair (`repair_regime`) was tried and failed, with the contradicting lessons it failed on. Finding nothing for some lessons means
    /// finding nothing for those lessons and more, so the search is not repeated until the contradictions themselves change. (Not saved: a search.)
    repair_failed: Vec<(String, Vec<(String, String)>)>,
    /// Which numbers of a question matter, when it holds more than the rules use (see `selector`); learned from lessons where the numbers used are known.
    selector: Option<crate::selector::Selector>,
    /// What its stated hit rates are multiplied by after it has checked itself against answers it was given and not taught from (`calibrate_guesses`): for a question
    /// it read as it was written, and for one it had to cut down to the numbers that matter. 1 until it has checked.
    guess_scale: [f32; 2],
    /// Lessons taken in by `learn_told` since the routing corrections were last learned.
    told_since_retrain: usize,
    /// Set while a lesson that was told is being taken in (`learn_told`): a unit that only guesses then is not re-fitted for every lesson.
    cheap_guessers: bool,
    /// For each repaired unit: (lessons when computed, the law it replaced re-derived from its other lessons, how many lessons told since have separated the two laws).
    old_law: std::cell::RefCell<HashMap<String, (usize, Option<Fitted>, usize)>>,
    /// (unit, its lesson count, the library) at the moment `learn` finished re-deciding that unit and left its state as it was: `learn_wondered` re-deciding the
    /// same unit from the same lessons and library would only repeat it
    just_relearned: Option<(usize, usize, String)>,
    /// What the selector was taught from (text, positions of the numbers used) and what it has been told since, so that it can be taught again.
    selector_examples: Vec<(String, Vec<usize>)>,
    /// For each unit, of the held-back lessons that were routed to it, the share that really belonged there (measured only when some unit is a
    /// partial program, whose guesses would otherwise state a rate that ignores how often the question is sent to the wrong unit).
    route_precision: Vec<f32>,
    /// Lessons that fit no unit it knows. They wait here until enough of them gather to form a unit of their own.
    pub pen: Vec<(String, String)>,
}

/// A question the baby would like answered, and why: the lessons it has leave two or more explanations equally good, and this is a
/// number on which they give different answers.
pub struct Wonder {
    pub unit: String,
    pub question: String,
    pub why: String,
}

pub struct Reply {
    /// Stated as fact: inside what it was taught, or has had checked. Every harness counts right/wrong on this alone.
    pub answer: Option<String>,
    /// Offered as a labeled guess when `answer` is None: a rule it trusts, applied beyond what it was taught or has had checked.
    /// Never counted as an answer; scored apart.
    pub guess: Option<Guess>,
    pub unit: Option<usize>,
    pub how: String,
}

/// What the baby thinks but has not checked: its rule's value outside the numbers it was taught, and how far outside.
#[derive(Clone, Debug)]
pub struct Guess {
    pub answer: String,
    /// How far past the taught range the furthest operand lies, in widths of that range (0.5 = half a range-width beyond the edge).
    pub stretch: f32,
    /// How often its own guesses this far out have turned out right, from the ones that were checked: (hits + 1) / (tried + 2).
    pub rate: f32,
    /// How many guesses this far out have been checked.
    pub tried: u32,
}

/// The distances (in widths of the taught range) that separate the buckets its guesses are sorted into.
pub const STRETCH_EDGES: [f32; 4] = [0.25, 0.5, 1.0, 2.0];

pub fn stretch_bucket(stretch: f32) -> usize {
    STRETCH_EDGES.iter().take_while(|e| stretch >= **e).count()
}

/// What happened when it was taught something after school.
pub struct Learned {
    pub unit: String,
    pub opened: bool,
    pub corrected: bool,
    pub was: Option<Mech>,
    pub now: Mech,
    pub held_back: f64,
    pub facts: usize,
    /// False when the unit could already work this lesson out: nothing was learned, nothing was stored.
    pub surprised: bool,
    /// The answer contradicts a rule the unit has good evidence for, and the baby is not sure the teacher is right: it was NOT learned, it is held
    /// until the same question is answered again (see `set_skeptical`).
    pub disputed: bool,
}

/// What the pen did with the lessons waiting in it.
pub struct PenReport {
    /// New units opened, with how many lessons each took.
    pub opened: Vec<(String, usize)>,
    /// Existing units that took lessons because the pen's group built exactly the same expression.
    pub joined: Vec<(String, usize)>,
    pub suggestions: Vec<String>,
    pub left: usize,
}

pub enum Outcome {
    Placed(Learned),
    Penned { pen: usize, report: PenReport },
}

/// What reorganizing changed.
pub struct Reorg {
    /// (kept unit, unit folded into it)
    pub merged: Vec<(String, String)>,
    /// (unit it was taken from, where the lessons went)
    pub split: Vec<(String, String, usize)>,
}

/// A digit string with a sign, decimal point, slash, comma or percent attached ("-3", "3.5", "1/2", "1,000", "50%"): not a plain whole
/// number. Returns the offending text.
pub fn unreadable_number(q: &str) -> Option<String> {
    let chars: Vec<char> = q.chars().collect();
    let digit = |i: usize| chars.get(i).map_or(false, |c| c.is_ascii_digit());
    let mut i = 0;
    while i < chars.len() {
        if !digit(i) {
            i += 1;
            continue;
        }
        let start = i;
        while digit(i) {
            i += 1;
        }
        let before = if start > 0 { chars[start - 1] } else { ' ' };
        let after = chars.get(i).copied().unwrap_or(' ');
        let separator = |c: char| matches!(c, '.' | ',' | '/' | ':');
        let bad_before = matches!(before, '-' | '+' | '$' | '#') || (separator(before) && start >= 2 && chars[start - 2].is_ascii_digit());
        let bad_after = after == '%' || (separator(after) && digit(i + 1));
        if bad_before || bad_after {
            let from = if bad_before { start - 1 } else { start };
            let to = if bad_after { (i + 2).min(chars.len()) } else { i };
            return Some(chars[from..to].iter().collect());
        }
    }
    None
}

/// A lesson's identity: the words in the order they were said. ("five minus two" and "two minus five" are different questions.)
fn lesson_key(q: &str) -> String {
    tokenize(q).join(" ")
}

pub struct Compacted {
    pub before: usize,
    pub after: usize,
}

/// The slow, exhaustive way to send a question to a unit (compare it with every lesson ever taught). Kept to measure what the
/// precomputed table saves.
pub struct NaiveRouter {
    shapes: Vec<Vec<Vec<String>>>,
    weights: Weights,
}

impl NaiveRouter {
    pub fn route_public(&self, shape: &[String], ledger: &mut Ledger) -> usize {
        self.route(shape, ledger)
    }

    fn route(&self, shape: &[String], ledger: &mut Ledger) -> usize {
        let mut best = (0, -1.0f32);
        for (u, shapes) in self.shapes.iter().enumerate() {
            for s in shapes {
                let (sim, steps) = self.weights.similarity_steps(shape, s);
                ledger.int_ops += steps;
                ledger.words_read += steps;
                ledger.macs += steps / 2;
                if sim > best.1 {
                    best = (u, sim);
                }
            }
        }
        best.0
    }
}

fn all_items_ref(units: &[SchoolUnit]) -> Vec<&Item> {
    units.iter().flat_map(|u| u.lessons.iter()).collect()
}

fn all_items(units: &[SchoolUnit]) -> Vec<Item> {
    units.iter().flat_map(|u| u.lessons.iter().cloned()).collect()
}

impl School {
    /// Teach a curriculum. `thrifty` skips the network wherever cheaper mechanisms already generalize.
    pub fn train(world: &World, seed: u64, thrifty: bool) -> School {
        let names: Vec<String> = UNITS.iter().map(|s| s.to_string()).collect();
        School::from_items(&world.train, names, Some(&world.reworded), seed, thrifty)
    }

    /// Teach any lessons that are already grouped into named units. `calibration` (rewordings of lessons) sets how decisive a
    /// question must be to be recognized; without it the floor comes from the lessons' own margins, more cautiously.
    pub fn from_items(train: &[Item], names: Vec<String>, calibration: Option<&[Item]>, seed: u64, thrifty: bool) -> School {
        let t0 = std::time::Instant::now();
        let baby = mechanisms::learn_items(train, names.len(), seed, thrifty);
        if std::env::var("BABY_PROF").is_ok() {
            eprintln!("PROF learn_items {:.1}s", t0.elapsed().as_secs_f32());
        }
        let library = baby.weights.lib.clone();
        let bar = baby.bar;
        let mut units = Vec::new();
        for (u, um) in baby.units.into_iter().enumerate() {
            let idx = Mech::ALL.iter().position(|m| *m == um.chosen).unwrap();
            let mut fitted = um.fitted;
            let state = fitted.swap_remove(idx);
            let lessons: Vec<Item> = train.iter().filter(|i| i.unit == u).cloned().collect();
            units.push(SchoolUnit {
                name: names[u].clone(),
                chosen: um.chosen,
                evidence: um.evidence,
                remember: Remembered::from_items(&lessons),
                lessons,
                state,
                exceptions: Vec::new(),
                refuted: false,
                signatures: Vec::new(),
            });
        }
        let line = NumberLine::from_counting(&counting_lessons());
        let mut weights = Weights::from(&all_items(&units));
        weights.lib = library;
        let routing = Routing::build(&units, &line);
        let library_seen = weights.lib.to_lines();
        let mut school = School { units, weights, routing, line, route_floor: f32::NEG_INFINITY, route_base: f32::NEG_INFINITY, route_ratio: 0.0, calibrated_lessons: 0, seed, library_seen, bar, careful: true, skeptical: true, disputed: Vec::new(), guess_record: [(0, 0); 5], repair_failed: Vec::new(), selector: None, guess_scale: [1.0, 1.0], told_since_retrain: 0, cheap_guessers: false, old_law: Default::default(), just_relearned: None, selector_examples: Vec::new(), route_precision: Vec::new(), pen: Vec::new() };
        // How decisive must a question be to count as a recognizable kind? Learned from rewordings of lessons it was taught.
        let own: Vec<&Item> = train.iter().collect();
        let (queries, safety): (Vec<&str>, f32) = match calibration {
            Some(c) => (c.iter().map(|i| i.query.as_str()).collect(), 0.25),
            None => (own.iter().map(|i| i.query.as_str()).collect(), 0.6),
        };
        let mut best: Vec<f32> = queries.iter().map(|q| school.route(q, &mut Ledger::default()).1).collect();
        best.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Keep 99% recognizable (the floor is a dial: lower recognizes more, risks more).
        let p1 = best[(best.len() as f32 * 0.01) as usize];
        school.route_floor = if calibration.is_some() { p1 - safety * p1.abs() } else { p1 * safety };
        school.route_base = school.route_floor;
        let loo = school.loo_margin_p1();
        school.route_ratio = if loo > 0.0 && school.route_floor > 0.0 { school.route_floor / loo } else { 0.0 };
        school.calibrated_lessons = school.total_lessons();
        // Now that every unit has been learned, and with it the vocabulary the later ones added, look again at the program units.
        // (If the library never gained anything, no unit was fitted with less than it has now: there is nothing to look at again. That is the case for units that
        // are partial programs, which are never vocabulary, and the review would only repeat their whole cross-validation.)
        school.library_seen = if school.weights.lib.is_empty() { school.weights.lib.to_lines() } else { String::new() };
        let t1 = std::time::Instant::now();
        school.review_programs();
        let t2 = std::time::Instant::now();
        school.settle_all_reach();
        if std::env::var("BABY_PROF").is_ok() {
            eprintln!("PROF review_programs {:.1}s, settle_all_reach {:.1}s", (t2 - t1).as_secs_f32(), t2.elapsed().as_secs_f32());
        }
        school
    }

    /// The unit a question most resembles, and by how much it beats the runner-up. (The margin, not the raw score: a raw score grows
    /// with the number of words, so a short question like "one plus zero" would look less recognizable than a long one.)
    pub fn route(&self, q: &str, ledger: &mut Ledger) -> (usize, f32) {
        let features = if self.routing.pure { routing_features(q, &self.line) } else { shape(q, &self.line) };
        let s = self.routing.scores(&features, ledger);
        School::best_and_margin(&s)
    }

    /// The best unit and its margin over the runner-up, in one pass (the earlier unit wins a tie).
    fn best_and_margin(s: &[f32]) -> (usize, f32) {
        let (mut best, mut second) = (0usize, None::<usize>);
        for u in 1..s.len() {
            if s[u] > s[best] {
                second = Some(best);
                best = u;
            } else if second.map_or(true, |k| s[u] > s[k]) {
                second = Some(u);
            }
        }
        let margin = match second {
            Some(k) => s[best] - s[k],
            None => s[best],
        };
        (best, margin)
    }

    fn total_lessons(&self) -> usize {
        self.units.iter().map(|u| u.lessons.len()).sum()
    }

    /// How clearly the baby recognizes a question it was NOT taught, estimated on its own lessons: the 1st-percentile margin of best unit over
    /// runner-up, each lesson routed with itself taken out of the table (a sample of at most 2,000, at an even stride, so the cost is bounded).
    fn loo_margin_p1(&self) -> f32 {
        let margins = self.loo_margins();
        if margins.is_empty() {
            return 0.0;
        }
        margins[((margins.len() as f32) * 0.01) as usize]
    }

    /// (for diagnosis) the sorted leave-one-out margins of a sample of the baby's own lessons
    pub fn loo_margins(&self) -> Vec<f32> {
        let all: Vec<(usize, &str)> = self.units.iter().enumerate().flat_map(|(u, unit)| unit.lessons.iter().map(move |l| (u, l.query.as_str()))).collect();
        if all.is_empty() {
            return Vec::new();
        }
        let stride = (all.len() / 2000).max(1);
        let mut margins: Vec<f32> =
            all.iter().step_by(stride).map(|(u, q)| School::best_and_margin(&self.routing.scores_without(&if self.routing.pure { routing_features(q, &self.line) } else { shape(q, &self.line) }, *u)).1).collect();
        margins.sort_by(|a, b| a.partial_cmp(b).unwrap());
        margins
    }

    /// Keep the recognition floor where it was relative to how clearly the baby recognizes what it was not taught. Learning in one unit lowers
    /// the margin of small units that share words with it (the right unit still wins, by less), and a floor fixed at teaching time then turns
    /// recognized questions into "no kind of lesson is like this". The floor is the ratio measured at teaching time times the leave-one-out
    /// margin now, and never above what it was at teaching time. Runs by itself after enough new lessons (see `maybe_recalibrate_route`), and in `reorganize`.
    pub fn recalibrate_route(&mut self) {
        if self.route_ratio > 0.0 {
            let now = self.loo_margin_p1();
            if now > 0.0 {
                self.route_floor = (self.route_ratio * now).min(self.route_base);
            }
        }
        self.calibrated_lessons = self.total_lessons();
    }

    /// After enough new lessons: 200, or 2 % of what it knew when the floor was last set, whichever is more. (The margins that shrink belong to
    /// small units whose neighbors grew, so the total can grow by only a few percent while they lose most of their margin: a threshold on total
    /// growth of a fifth never fired.)
    fn maybe_recalibrate_route(&mut self) {
        let added = self.total_lessons().saturating_sub(self.calibrated_lessons);
        if self.route_ratio > 0.0 && added >= 200.max(self.calibrated_lessons / 50) {
            self.recalibrate_route();
        }
    }

    /// (for diagnosis) one unit's competing explanations
    pub fn describe_unit_open(&self, u: usize) -> String {
        self.units[u].describe_open(&self.weights.lib)
    }

    /// Decline to extrapolate unchecked (see the `careful` field).
    pub fn set_careful(&mut self, careful: bool) {
        self.careful = careful;
    }

    /// The total weight of a question's non-number words under the unit's rule or selection: the function the question asks for. None for
    /// other mechanisms (a program has one function, so every lesson shows where its question lies).
    fn signature(state: &Fitted, line: &NumberLine, query: &str) -> Option<[f32; 3]> {
        let words: Vec<String> = tokenize(query).into_iter().filter(|t| line.value(t).is_none()).collect();
        match state {
            Fitted::Rule(r) => {
                let mut s = [0.0f32; 3];
                for w in &words {
                    if let Some(o) = r.op_of(w) {
                        for k in 0..3 {
                            s[k] += o[k];
                        }
                    }
                }
                Some(s)
            }
            Fitted::Select(sel) => Some([words.iter().filter_map(|w| sel.score_of(w)).sum(), 0.0, 0.0]),
            _ => None,
        }
    }

    /// Do two signatures ask for the same function? (A rule: the same total weights within the rule's own engagement margin; a selection: both
    /// want the bigger or both the smaller.)
    fn same_function(a: &[f32; 3], b: &[f32; 3], select: bool) -> bool {
        if select {
            (a[0] > 0.0) == (b[0] > 0.0)
        } else {
            (0..3).all(|k| (a[k] - b[k]).abs() <= 0.25)
        }
    }

    /// The lessons of unit `u` that ask for the same function as this question (indices; empty means all of them). This is what decides which
    /// lessons show where the question lies in the range taught. It used to be "lessons sharing a word that is not template glue", and what counts
    /// as glue moves as facts are added: a template word like "equals" stopped being glue at the 32nd fact and the check answers, phrased without it,
    /// stopped counting, so coverage FELL as the baby learned.
    fn reach_lessons(&self, u: usize, tokens: &[String]) -> Vec<usize> {
        let unit = &self.units[u];
        let Some(q) = School::signature(&unit.state, &self.line, &tokens.join(" ")) else { return Vec::new() };
        let select = matches!(unit.state, Fitted::Select(_));
        let hits: Vec<usize> = (0..unit.signatures.len()).filter(|&i| School::same_function(&unit.signatures[i], &q, select)).collect();
        if hits.len() == unit.lessons.len() { Vec::new() } else { hits }
    }

    /// Work out which lessons of unit `u` its current rule does not explain, and whether one of them lies outside what the others span (the rule
    /// has been shown wrong out there). Called whenever a unit is (re)fitted. The tolerance for a few unexplained lessons is right for wrong
    /// labels inside the taught range, and wrong for a correction of a guess outside it: such a lesson must not widen where the rule may speak.
    fn settle_reach(&mut self, u: usize) {
        let unit = &self.units[u];
        let numeric = matches!(unit.state, Fitted::Rule(_) | Fitted::Select(_) | Fitted::Program(Some(_)));
        let signatures: Vec<[f32; 3]> = unit.lessons.iter().map(|l| School::signature(&unit.state, &self.line, &l.query).unwrap_or([0.0; 3])).collect();
        let has_signatures = matches!(unit.state, Fitted::Rule(_) | Fitted::Select(_));
        let select = matches!(unit.state, Fitted::Select(_));
        let mut exceptions: Vec<usize> = Vec::new();
        if numeric {
            for (i, l) in unit.lessons.iter().enumerate() {
                if predict(&unit.state, &l.query, &self.weights).as_deref() != Some(l.answer.as_str()) {
                    exceptions.push(i);
                }
            }
        }
        let value = |w: &str| self.line.value(w);
        // lessons a repaired program was built around are told, not proven: they are left out of the range the program may speak in
        let unproven: Vec<usize> = match &unit.state {
            Fitted::Program(Some(p)) if !p.repaired.is_empty() => (0..unit.lessons.len()).filter(|&i| p.repaired.contains(&unit.lessons[i].query)).collect(),
            _ => Vec::new(),
        };
        let refuted = exceptions.iter().any(|&i| {
            let tokens = tokenize(&unit.lessons[i].query);
            let candidates: Vec<usize> = if has_signatures {
                let hits: Vec<usize> = (0..signatures.len()).filter(|&k| School::same_function(&signatures[k], &signatures[i], select)).collect();
                if hits.len() == signatures.len() { Vec::new() } else { hits }
            } else {
                Vec::new()
            };
            let mut skip: Vec<usize> = exceptions.iter().chain(&unproven).copied().collect();
            skip.sort_unstable();
            let hull = unit.remember.operand_hull(&candidates, &value, &skip);
            let asked: Vec<f32> = tokens.iter().filter_map(|t| value(t)).collect();
            // outside the box of what the other lessons span, or inside it at a corner that no other lesson is at least as large as (a staircase, as
            // the reach is): a wrap that shows itself only with both numbers large lies inside the box
            asked.iter().enumerate().any(|(k, v)| hull.get(k).map_or(false, |&(lo, hi)| *v < lo || *v > hi)) || unit.remember.uncovered_by(&candidates, &value, &skip, &asked).is_some()
        });
        // (what is left out of the reach is recorded with the exceptions: both are lessons that do not vouch for their neighbors)
        for &i in &unproven {
            if !exceptions.contains(&i) {
                exceptions.push(i);
            }
        }
        exceptions.sort_unstable(); // (the reach functions search it)
        let unit = &mut self.units[u];
        unit.exceptions = exceptions;
        unit.refuted = refuted;
        unit.signatures = if has_signatures { signatures } else { Vec::new() };
    }

    /// Measure how often a question routed to each unit really belongs there, by routing every lesson with the table built from the others.
    /// Only when some unit is a partial program; elsewhere routing is exact and precision is taken as 1.
    /// A rebuilt law (a repaired program) does not yet get to overrule the law it replaced. That law is a competing explanation, and the project's rule is to
    /// state only where competing explanations agree: until `RETIRE_AFTER` lessons told AFTER the repair have separated the two (the old one wrong, the new one
    /// right), anything the new law says that the old law contradicts is only a guess. Without this, one wrong lesson beyond the taught range (a persistent
    /// teacher's) could be rebuilt into a law, and answers that merely agree with both laws then vouched for it where they differ.
    fn repair_veto(&self, u: usize, q: &str, said: &str) -> u8 {
        let unit = &self.units[u];
        let Fitted::Program(Some(p)) = &unit.state else { return 0 };
        let mech = match p.before.as_deref() {
            Some("rule") => Mech::Rule,
            Some("select") => Mech::Select,
            Some("program") => Mech::Program,
            _ => return 0,
        };
        let n = unit.lessons.len();
        let mut cache = self.old_law.borrow_mut();
        let entry = cache.entry(unit.name.clone()).or_insert((usize::MAX, None, 0));
        if entry.0 != n {
            let others: Vec<Item> = unit.lessons.iter().filter(|l| !p.repaired.contains(&l.query)).cloned().collect();
            let old = fit(mech, &others, &self.weights, self.seed);
            let support = others
                .iter()
                .filter(|l| predict(&old, &l.query, &self.weights).as_deref() != Some(l.answer.as_str()) && predict(&unit.state, &l.query, &self.weights).as_deref() == Some(l.answer.as_str()))
                .count();
            *entry = (n, Some(old), support);
        }
        if entry.2 >= RETIRE_AFTER {
            return 0;
        }
        match entry.1.as_ref().and_then(|o| predict(o, q, &self.weights)) {
            Some(o) if o != said => 1,
            Some(_) => 0,
            // the law it replaced has nothing to say here (a result it cannot represent, say): that is no endorsement
            None => 2,
        }
    }

    fn settle_route_precision(&mut self) {
        let any_partial = self.units.iter().any(|u| matches!(&u.state, Fitted::Program(Some(p)) if p.partial.is_some()));
        if !any_partial || self.units.len() < 2 {
            self.route_precision = Vec::new();
            self.routing.delta.clear();
            return;
        }
        let n = self.units.len();
        let shapes: Vec<Vec<(usize, Vec<String>)>> = self.units.iter().enumerate().map(|(u, unit)| unit.lessons.iter().map(|l| (u, routing_features(&l.query, &self.line))).collect()).collect();
        // five-fold routing of its own lessons, by counting alone and with the corrections learned from the training folds' mistakes
        let (mut routed, mut hit) = (vec![0u32; n], vec![0u32; n]);
        let (mut routed_r, mut hit_r) = (vec![0u32; n], vec![0u32; n]);
        for fold in 0..ROUTE_FOLDS {
            let mut table = Routing::build_without(&self.units, &self.line, fold);
            let mut train: Vec<(usize, Vec<String>)> = Vec::new();
            let mut test: Vec<(usize, Vec<String>)> = Vec::new();
            let mut position = 0usize;
            for unit_shapes in &shapes {
                for sample in unit_shapes {
                    if position % ROUTE_FOLDS == fold { test.push(sample.clone()) } else { train.push(sample.clone()) }
                    position += 1;
                }
            }
            let argmax = |scores: &[f32]| scores.iter().enumerate().fold(0usize, |b, (i, s)| if *s > scores[b] { i } else { b });
            for (u, sh) in &test {
                let best = argmax(&table.scores(sh, &mut Ledger::default()));
                routed[best] += 1;
                if best == *u {
                    hit[best] += 1;
                }
            }
            table.pure = true;
            table.refine(&train, ROUTE_EPOCHS);
            for (u, sh) in &test {
                let best = argmax(&table.scores(sh, &mut Ledger::default()));
                routed_r[best] += 1;
                if best == *u {
                    hit_r[best] += 1;
                }
            }
        }
        // Counting is kept unless the corrections route measurably more of the held-out lessons to their own unit.
        let total = |h: &[u32]| h.iter().sum::<u32>();
        if std::env::var("ROUTE_DEBUG").is_ok() {
            eprintln!("ROUTE5 counting {}/{}  with corrections {}/{}", total(&hit), total(&routed), total(&hit_r), total(&routed_r));
        }
        let use_corrections = total(&hit_r) > total(&hit) && (total(&hit) as f32) < 0.95 * total(&routed) as f32;
        let (routed, hit) = if use_corrections { (routed_r, hit_r) } else { (routed, hit) };
        self.routing.delta.clear();
        self.routing.pure = false;
        if use_corrections {
            let all: Vec<(usize, Vec<String>)> = shapes.into_iter().flatten().collect();
            self.routing.pure = true;
            self.routing.refine(&all, ROUTE_EPOCHS);
        }
        self.route_precision = (0..n).map(|u| (hit[u] as f32 + 1.0) / (routed[u] as f32 + 2.0)).collect();
    }

    fn settle_all_reach(&mut self) {
        for u in 0..self.units.len() {
            self.settle_reach(u);
        }
        self.settle_route_precision();
    }

    /// (for tests) the cached exceptions and refutations of every unit
    #[cfg(test)]
    fn reach_snapshot(&self) -> Vec<(Vec<usize>, bool)> {
        self.units.iter().map(|u| (u.exceptions.clone(), u.refuted)).collect()
    }

    /// (for diagnosis) a unit's lessons, its exceptions, whether it is refuted, and the reach it computes for a question
    pub fn debug_reach(&self, u: usize, q: &str) -> String {
        let unit = &self.units[u];
        let value = |w: &str| self.line.value(w);
        let tokens = tokenize(q);
        let cands = self.reach_lessons(u, &tokens);
        let hull = unit.remember.operand_hull(&cands, &value, &unit.exceptions);
        let unskipped = unit.remember.operand_hull(&cands, &value, &[]);
        let ex: Vec<&str> = unit.exceptions.iter().take(3).map(|&i| unit.lessons[i].query.as_str()).collect();
        format!("{} lessons, {} exceptions {:?}, refuted {}, hull {:?} (without skipping exceptions {:?})", unit.lessons.len(), unit.exceptions.len(), ex, unit.refuted, hull, unskipped)
    }

    /// Be skeptical of the teacher: an answer to one of its own checks that contradicts a rule it has good evidence for is held, not learned, until
    /// the same question is answered again with the same answer (then it is believed, and the rule is refuted properly) or with the rule's answer
    /// (then the first one was a slip). On by default; costs one more question per contradiction.
    pub fn set_skeptical(&mut self, skeptical: bool) {
        self.skeptical = skeptical;
        if !skeptical {
            self.disputed.clear();
        }
    }

    /// Answers currently held for a second opinion.
    pub fn disputed_count(&self) -> usize {
        self.disputed.len()
    }

    /// How many units have been shown wrong by a lesson outside what their rule explains (they no longer guess there).
    pub fn refuted_units(&self) -> usize {
        self.units.iter().filter(|u| u.refuted).count()
    }

    /// The skeptic's weighing of an answer against the unit's own rule. None: nothing to doubt, learn it as usual. Some: it was held.
    fn weigh_against_rule(&mut self, u: usize, query: &str, answer: &str) -> Option<Learned> {
        let unit = &self.units[u];
        let settled = matches!(unit.chosen, Mech::Rule | Mech::Select | Mech::Program)
            && !matches!(&unit.state, Fitted::Program(Some(p)) if p.partial.is_some())
            && unit.best_evidence() >= mechanisms::reliable()
            && unit.facts() >= 8
            && !unit.refuted;
        if !settled {
            return None;
        }
        let predicted = self.unit_predicts(u, query)?;
        let name = self.units[u].name.clone();
        let key = lesson_key(query);
        let pos = self.disputed.iter().position(|d| d.0 == name && lesson_key(&d.1) == key);
        if predicted == answer {
            // it agrees with the rule: any doubt held about this question was a slip
            if let Some(p) = pos {
                self.disputed.remove(p);
            }
            return None;
        }
        match pos {
            Some(p) if self.disputed[p].2 == answer => {
                // said twice: believed
                self.disputed.remove(p);
                return None;
            }
            Some(p) => self.disputed[p].2 = answer.to_string(),
            None => self.disputed.push((name, query.to_string(), answer.to_string())),
        }
        let unit = &self.units[u];
        Some(Learned { unit: unit.name.clone(), opened: false, corrected: false, was: Some(unit.chosen), now: unit.chosen, held_back: unit.best_evidence(), facts: unit.facts(), surprised: false, disputed: true })
    }

    /// Is any operand of this question outside the range unit `u` was taught (or has had confirmed) for the operation it names? Then why.
    fn beyond_reach(&self, u: usize, tokens: &[String]) -> Option<(String, f32)> {
        let unit = &self.units[u];
        let value = |w: &str| self.line.value(w);
        let words = self.reach_lessons(u, tokens);
        let hull = unit.remember.operand_hull(&words, &value, &unit.exceptions);
        let asked: Vec<f32> = tokens.iter().filter_map(|t| value(t)).collect();
        let mut found: Option<(String, f32)> = None;
        for (i, v) in asked.iter().enumerate() {
            if let Some(&(lo, hi)) = hull.get(i) {
                if *v < lo || *v > hi {
                    let stretch = (if *v > hi { *v - hi } else { lo - *v }) / (hi - lo + 1.0);
                    if found.as_ref().map_or(true, |f| stretch > f.1) {
                        found = Some((
                            format!("for this kind I was only taught (or have checked) numbers from {lo} to {hi}, and this asks about {v}: I won't state it as fact unchecked (see: baby wonder --check)"),
                            stretch,
                        ));
                    }
                }
            }
        }
        if found.is_none() && !asked.is_empty() {
            // inside every operand's range, but is the corner it names anywhere near something taught or checked?
            if let Some(short) = unit.remember.uncovered_by(&words, &value, &unit.exceptions, &asked) {
                let width = hull.iter().map(|(lo, hi)| hi - lo + 1.0).fold(1.0f32, f32::max);
                found = Some((
                    format!("each number is within what I was taught (or have checked), but never together like this (nothing taught is as large in every place at once): I won't state it as fact unchecked (see: baby wonder --check)"),
                    short / width,
                ));
            }
        }
        found
    }

    /// (for diagnosis) the library as text
    pub fn library_text(&self) -> String {
        self.weights.lib.to_lines()
    }

    /// Its record as a guesser, per distance bucket: (right, wrong).
    pub fn guess_record(&self) -> [(u32, u32); 5] {
        self.guess_record
    }

    /// The reliability bar this baby measured for its lessons.
    pub fn bar(&self) -> f64 {
        self.bar
    }

    /// Words it knows (from any lesson, or the number line).
    pub fn knows(&self, w: &str) -> bool {
        self.weights.knows(w) || self.line.value(w).is_some()
    }

    pub fn known_words(&self) -> Vec<String> {
        let mut v = self.weights.words();
        v.extend(counting_lessons().into_iter().flat_map(|(a, b)| [a, b]));
        v.sort();
        v.dedup();
        v
    }

    pub fn word_weight(&self, w: &str) -> f32 {
        self.weights.w(w)
    }

    pub fn naive_router(&self) -> NaiveRouter {
        let shapes: Vec<Vec<Vec<String>>> =
            self.units.iter().map(|u| u.lessons.iter().map(|l| shape(&l.query, &self.line)).collect()).collect();
        let weights = Weights::from_lists(shapes.iter().enumerate().flat_map(|(u, qs)| qs.iter().map(move |q| (u, q.clone()))).collect());
        NaiveRouter { shapes, weights }
    }

    /// Give the baby a way to tell which numbers of a question matter (see `selector`).
    /// Where does a question and its answer belong? The unit whose program turns some numbers of the question into this answer, and which numbers (their
    /// positions among the question's numbers). Only when that is unambiguous: one unit and one choice of numbers, or, if several fit, the one the selector
    /// would have chosen. None if no program reproduces the answer from any numbers of the question, or the choice is not clear.
    pub fn place_told(&self, query: &str, answer: &str) -> Option<(usize, Vec<usize>)> {
        let truth = self.line.value(answer)?;
        let tokens = tokenize(query);
        let numbers: Vec<f32> = tokens.iter().filter_map(|t| self.line.value(t)).collect();
        let mut fits: Vec<(usize, Vec<usize>)> = Vec::new();
        for (u, unit) in self.units.iter().enumerate() {
            let Fitted::Program(Some(p)) = &unit.state else { continue };
            match p.arity() {
                1 => {
                    for i in 0..numbers.len() {
                        let v = p.expr.eval(numbers[i], 0.0);
                        if v.is_finite() && (v - truth).abs() < 0.01 {
                            fits.push((u, vec![i]));
                        }
                    }
                }
                2 => {
                    for i in 0..numbers.len() {
                        for j in (i + 1)..numbers.len() {
                            let v = p.expr.eval(numbers[i], numbers[j]);
                            if v.is_finite() && (v - truth).abs() < 0.01 {
                                fits.push((u, vec![i, j]));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        match fits.len() {
            0 => None,
            1 => fits.pop(),
            _ => {
                // several: the choice the selector would have made, if it is among them
                let chosen: Option<Vec<usize>> = self.selector.as_ref().filter(|_| numbers.len() > 2).map(|s| {
                    let scores = s.scores(query, &self.line);
                    let mut ranked: Vec<usize> = (0..scores.len()).collect();
                    ranked.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal).then(b.cmp(&a)));
                    let mut top: Vec<usize> = ranked.into_iter().take(2).collect();
                    top.sort();
                    top
                });
                let pick: Vec<&(usize, Vec<usize>)> = fits.iter().filter(|(_, pair)| Some(pair) == chosen.as_ref()).collect();
                if pick.len() == 1 { Some(pick[0].clone()) } else { None }
            }
        }
    }

    /// Be told the answer to a question and learn from it where it belongs (`place_told`): the question, cut down to the numbers whose program reproduces
    /// the answer, goes to that unit, and which numbers mattered goes to the selector. An answer that cannot be placed is not taken in (a lesson its unit
    /// cannot explain would only weaken the program it is filed under): this returns an error saying so. Every `ROUTE_RETRAIN_EVERY` placed lessons the
    /// routing corrections and the selector are learned again from everything it knows.
    pub fn learn_told(&mut self, query: &str, answer: &str) -> Result<Learned, String> {
        if let Some(bad) = unreadable_number(query) {
            return Err(format!("I only read plain whole numbers, not '{bad}'"));
        }
        let Some((unit, pair)) = self.place_told(query, answer) else {
            return Err("no kind of question I know turns the numbers of this one into that answer, or it is not clear which numbers: I did not take it in".into());
        };
        let name = self.units[unit].name.clone();
        let focused = crate::selector::keep_only(query, &pair, &self.line);
        self.cheap_guessers = true;
        let learned = self.learn(&focused, answer, Some(&name));
        self.cheap_guessers = false;
        let learned = learned?;
        if self.selector.is_some() {
            self.selector_examples.push((query.to_string(), pair));
        }
        self.told_since_retrain += 1;
        if self.told_since_retrain >= ROUTE_RETRAIN_EVERY {
            self.told_since_retrain = 0;
            if self.selector.is_some() {
                self.selector = Some(crate::selector::Selector::train(&self.selector_examples, &self.line));
            }
            self.settle_route_precision();
        }
        Ok(learned)
    }

    pub fn guess_scales(&self) -> [f32; 2] {
        self.guess_scale
    }

    pub fn set_guess_scales(&mut self, scale: [f32; 2]) {
        self.guess_scale = scale;
    }

    pub fn set_selector(&mut self, selector: crate::selector::Selector, examples: Vec<(String, Vec<usize>)>) {
        self.selector = Some(selector);
        self.selector_examples = examples;
    }

    /// The question as the baby reads it: with a selector, a question holding more than two numbers keeps the two that matter and blanks the rest.
    pub fn focus(&self, q: &str) -> String {
        self.selector.as_ref().and_then(|s| s.focus(q, &self.line, 2)).unwrap_or_else(|| q.to_string())
    }

    /// (for the real-text bridge) the expression of a unit that is a program, and whether it only fits most of its lessons.
    pub fn unit_expression(&self, name: &str) -> Option<crate::program::Expr> {
        match &self.units.iter().find(|u| u.name == name)?.state {
            Fitted::Program(Some(p)) => Some(p.expr.clone()),
            _ => None,
        }
    }

    pub fn number_line(&self) -> &NumberLine {
        &self.line
    }

    pub fn ask(&self, q: &str) -> Reply {
        self.ask_with(q, &mut Ledger::default(), None)
    }

    /// Answer, counting the work. With a `NaiveRouter` it does everything the slow way (and skips the "recognizable kind" check,
    /// which belongs to the table), so the two can be compared on cost.
    pub fn ask_with(&self, q: &str, ledger: &mut Ledger, naive: Option<&NaiveRouter>) -> Reply {
        self.ask_in(q, ledger, naive, None)
    }

    /// (diagnosis) Answer as if the question had been routed to the unit called `unit`: separates how good a unit's way of learning is from
    /// how well the baby recognizes which unit a question belongs to.
    pub fn ask_as(&self, q: &str, unit: &str) -> Option<Reply> {
        let u = self.units.iter().position(|x| x.name == unit)?;
        Some(self.ask_in(q, &mut Ledger::default(), None, Some(u)))
    }

    fn ask_in(&self, q: &str, ledger: &mut Ledger, naive: Option<&NaiveRouter>, forced: Option<usize>) -> Reply {
        let mut reply = self.ask_core(q, ledger, naive, forced);
        if let Some(g) = reply.guess.as_mut() {
            g.rate *= self.guess_scale[usize::from(self.focus(q) != q)];
        }
        reply
    }

    /// Check its stated hit rates against questions whose answers it was given but not taught from: ask each, and where it offers a guess note whether it was right.
    /// Afterwards every guess states the rate it really achieved (never above what it stated before), separately for questions it read as written and for
    /// ones it had to cut down to the numbers that matter. Needs at least ten guesses in a class to change that class's scale.
    pub fn calibrate_guesses(&mut self, held_back: &[(String, String)]) {
        self.guess_scale = [1.0, 1.0];
        let (mut stated, mut hits, mut n) = ([0.0f32; 2], [0u32; 2], [0u32; 2]);
        for (q, truth) in held_back {
            let class = usize::from(self.focus(q) != *q);
            if let Some(g) = self.ask(q).guess {
                stated[class] += g.rate;
                n[class] += 1;
                if g.answer == *truth {
                    hits[class] += 1;
                }
            }
        }
        for c in 0..2 {
            if n[c] >= 10 && stated[c] > 0.0 {
                let realized = (hits[c] as f32 + 1.0) / (n[c] as f32 + 2.0);
                self.guess_scale[c] = (realized / (stated[c] / n[c] as f32)).min(1.0);
            }
        }
    }

    fn ask_core(&self, q: &str, ledger: &mut Ledger, naive: Option<&NaiveRouter>, forced: Option<usize>) -> Reply {
        // Numbers are plain whole numbers. A sign, a decimal point, a fraction slash, a thousands comma or a percent attached to a digit
        // would be silently dropped by reading words ("-3" would become 3), so such a question is refused rather than misread.
        if let Some(bad) = unreadable_number(q) {
            return Reply { guess: None, answer: None, unit: None, how: format!("I only read plain whole numbers, not '{bad}' (signs, decimals, fractions, percentages)") };
        }
        // A question with more numbers than a rule uses: keep the ones that matter, blank the rest (see `selector`).
        let focused = self.focus(q);
        let q: &str = &focused;
        let tokens = tokenize(q);
        ledger.int_ops += tokens.len() as u64;
        ledger.words_read += tokens.len() as u64;
        // A word it has never met cannot be part of something it knows how to answer.
        // (Only a partial program, whose answers are guesses anyway, may go on with a word it has never met: it was never required to know every word.)
        let unmet: Vec<String> = tokens.iter().filter(|w| !self.knows(w)).cloned().collect();
        let refuse_unmet = || Reply { guess: None, answer: None, unit: None, how: format!("I haven't met the word{} {}", if unmet.len() > 1 { "s" } else { "" }, unmet.join(", ")) };
        let partial_unit = |u: usize| matches!(&self.units[u].state, Fitted::Program(Some(p)) if p.partial.is_some());
        let u = match (forced, naive) {
            (Some(u), _) => u,
            (None, Some(r)) => r.route(&shape(q, &self.line), ledger),
            (None, None) => {
                if !unmet.is_empty() && !self.units.iter().any(|u| matches!(&u.state, Fitted::Program(Some(p)) if p.partial.is_some())) {
                    return refuse_unmet();
                }
                let (u, score) = self.route(q, ledger);
                if score < self.route_floor {
                    return Reply { guess: None, answer: None, unit: None, how: "no kind of lesson I was taught is like this question".into() };
                }
                u
            }
        };
        if !unmet.is_empty() && !partial_unit(u) {
            return refuse_unmet();
        }
        let unit = &self.units[u];
        let said = |what: &str| format!("unit '{}', {what}", unit.name);
        let n = tokens.len() as u64;
        match &unit.state {
            Fitted::Network(m) => {
                let c = m.net.cfg;
                ledger.macs += (c.hidden * c.input() + c.classes * c.hidden) as u64;
                ledger.words_read += (c.hidden * c.input() + c.classes * c.hidden) as u64;
                ledger.touch(4 * c.n_params());
                let mut s = Scratch::new(&c);
                match m.ask(q, 1, &mut s).into_iter().next() {
                    Some((a, p)) if p >= 0.9 => Reply { guess: None, answer: Some(a), unit: Some(u), how: said(&format!("a small network ({:.0}% sure)", p * 100.0)) },
                    _ => Reply { guess: None, answer: None, unit: Some(u), how: said("a small network, but it is not sure enough") },
                }
            }
            state => {
                let answer = match state {
                    Fitted::Remember(_) => unit.remember.recall_counted(q, &self.weights, ledger, naive.is_none()),
                    other => {
                        match other {
                            Fitted::Rule(_) => ledger.macs += 3 * n + 11,
                            Fitted::Select(_) => ledger.macs += 2 * n + 4,
                            Fitted::Program(Some(p)) => ledger.macs += p.expr.size() as u64 + n,
                            _ => {}
                        }
                        ledger.words_read += n + 11;
                        let worked_out = predict(other, q, &self.weights);
                        // Two witnesses: if this exact question was taught, and what was taught for that fact (the vote of its phrasings) disagrees with the rule, one of
                        // them is wrong and nothing says which (a skewed fit, or a wrong lesson). Decline rather than pick. Clean lessons
                        // never disagree, so this costs nothing there.
                        match (&worked_out, naive.is_none()) {
                            (Some(a), true) => match unit.remember.exact(q).and_then(|_| unit.remember.recall_counted(q, &self.weights, &mut Ledger::default(), true)) {
                                // Out where a lesson has already shown the rule wrong, what it was told is the better witness.
                                Some(m) if &m != a && self.careful && unit.refuted && self.beyond_reach(u, &tokens).is_some() => {
                                    return Reply {
                                        guess: None,
                                        answer: Some(m),
                                        unit: Some(u),
                                        how: said(&format!("recalled from what I was told: my rule says {a} here, but a lesson out here has already shown my rule wrong")),
                                    };
                                }
                                Some(m) if &m != a => {
                                    return Reply { guess: None,
                                        answer: None,
                                        unit: Some(u),
                                        how: said(&format!("my rule says {a}, but I was taught {m} for this very question: they disagree, so I won't guess")),
                                    };
                                }
                                _ => worked_out,
                            },
                            _ => worked_out,
                        }
                    }
                };
                // A program that fits most but not all of what it was taught is never a fact: its answer is a guess, as sure as it was on what it held back.
                if let (Some(a), Fitted::Program(Some(p))) = (&answer, &unit.state) {
                    if p.partial.is_some() {
                        return Reply {
                            guess: Some(Guess {
                                answer: a.clone(),
                                stretch: 0.0,
                                // right on what it held back, times how often a question sent here really belongs here
                                rate: unit.best_evidence() as f32 * self.route_precision.get(u).copied().unwrap_or(if self.route_precision.is_empty() { 1.0 } else { 0.5 }),
                                tried: unit.facts() as u32,
                            }),
                            answer: None,
                            unit: Some(u),
                            how: said(&format!(
                                "worked out by an expression that fits most but not all of what I was taught, {} (right on {:.0}% of the facts I held back, and a question sent to this unit really belongs to it {:.0}% of the time): only a guess",
                                unit.program().unwrap_or_default(),
                                100.0 * unit.best_evidence(),
                                100.0 * self.route_precision.get(u).copied().unwrap_or(1.0)
                            )),
                        };
                    }
                }
                if let (Some(a), Fitted::Program(Some(p))) = (&answer, &unit.state) {
                    // (a question it was told, exactly, is a fact whatever the old law says: that is what it was told)
                    let vetoed = if !p.repaired.is_empty() && p.partial.is_none() && unit.remember.exact(q).as_deref() != Some(a.as_str()) { self.repair_veto(u, q, a) } else { 0 };
                    if vetoed != 0 {
                        let how = if vetoed == 1 {
                            "this unit's rule was rebuilt from a correction, and the rule it replaced says something else here"
                        } else {
                            "this unit's rule was rebuilt from a correction, and the rule it replaced has nothing to say here"
                        };
                        if unit.refuted || vetoed == 2 {
                            // (a unit that has been shown wrong somewhere does not even guess; nor does a rebuilt rule where the earlier one is silent)
                            return Reply { guess: None, answer: None, unit: Some(u), how: said(&format!("{how}: I will not say")) };
                        }
                        let (hit, miss) = self.guess_record[stretch_bucket(0.3)];
                        return Reply {
                            guess: Some(Guess { answer: a.clone(), stretch: 0.3, rate: (hit as f32 + 1.0) / ((hit + miss) as f32 + 2.0), tried: hit + miss }),
                            answer: None,
                            unit: Some(u),
                            how: said(&format!("{how}: only a guess until more lessons have settled which is right")),
                        };
                    }
                }
                if self.careful && answer.is_some() && matches!(unit.chosen, Mech::Rule | Mech::Select | Mech::Program) {
                    if let (Some((why, stretch)), Some(a)) = (self.beyond_reach(u, &tokens), answer.as_ref()) {
                        // a question it was told, and the unit's own working agrees with what it was told: that is a fact, whatever the reach
                        if unit.remember.exact(q).is_some() && unit.remember.recall_counted(q, &self.weights, &mut Ledger::default(), true).as_ref() == Some(a) {
                            return Reply { guess: None, answer: Some(a.clone()), unit: Some(u), how: said("recalled from what I was told, and my working agrees") };
                        }
                        if unit.refuted {
                            return Reply {
                                guess: None,
                                answer: None,
                                unit: Some(u),
                                how: said(&format!("{why}; and a lesson out there has already shown my rule wrong, so I won't even guess")),
                            };
                        }
                        let (hit, miss) = self.guess_record[stretch_bucket(stretch)];
                        // (a repaired program is a new hypothesis: the miss that made it is not its own, so it is not silenced by it, but it states the
                        // record's low rate all the same)
                        let repaired = matches!(&unit.state, Fitted::Program(Some(p)) if !p.repaired.is_empty());
                        if miss > hit && !repaired {
                            return Reply {
                                guess: None,
                                answer: None,
                                unit: Some(u),
                                how: said(&format!("{why}; and my guesses this far out have been wrong more often than right ({hit} right, {miss} wrong), so I won't guess")),
                            };
                        }
                        let rate = (hit as f32 + 1.0) / ((hit + miss) as f32 + 2.0);
                        return Reply { guess: Some(Guess { answer: a.clone(), stretch, rate, tried: hit + miss }), answer: None, unit: Some(u), how: said(&why) };
                    }
                }
                match answer {
                    Some(a) => {
                        let how = match unit.chosen {
                            Mech::Remember if unit.memorize_only() => {
                                said("recalled, not worked out: nothing I tried could work these out, so I only remember what I was told")
                            }
                            Mech::Remember => said("recalled from what I was taught"),
                            Mech::Rule => said(&format!("worked out by a learned rule ({:.0}% on facts I held back)", 100.0 * unit.best_evidence())),
                            Mech::Select => said(&format!("worked out by picking the bigger or smaller ({:.0}% on facts I held back)", 100.0 * unit.best_evidence())),
                            Mech::Program => said(&format!(
                                "worked out by an expression I built, {} ({:.0}% on facts I held back)",
                                unit.program().unwrap_or_default(),
                                100.0 * unit.best_evidence()
                            )),
                            Mech::Network => said("a small network"),
                        };
                        Reply { guess: None, answer: Some(a), unit: Some(u), how }
                    }
                    None => {
                        let why = match unit.chosen {
                            Mech::Remember => "I only memorize this kind of thing and nothing I was taught is close enough",
                            _ => "I couldn't work it out: it has a word or a shape I haven't learned how to handle",
                        };
                        Reply { guess: None, answer: None, unit: Some(u), how: said(why) }
                    }
                }
            }
        }
    }

    // ---- learning after school ----

    /// Recompute the weights and the routing table from every lesson. Needed when lessons move between units (merge, split, compact);
    /// taking on one more lesson uses `add_to_tables` instead.
    fn rebuild_tables(&mut self) {
        self.renumber();
        let library = std::mem::take(&mut self.weights.lib);
        self.weights = Weights::from_lists(
            self.units.iter().enumerate().flat_map(|(u, unit)| unit.lessons.iter().map(move |l| (u, tokenize(&l.query)))).collect(),
        );
        self.weights.lib = library;
        self.routing = Routing::build(&self.units, &self.line);
    }

    /// One more lesson in unit `u`: update the weights and the routing table by its own words only.
    fn add_to_tables(&mut self, u: usize, query: &str) {
        self.weights.add(u, &tokenize(query));
        self.routing.add_lesson(u, &shape(query, &self.line));
    }

    /// A numeric unit whose rule has been shown wrong outside what it explains is given one chance to learn the new regime: is there an exact
    /// program, with the numbers the contradicting lessons point to (their answers; how far the old rule was), that fits EVERY lesson? A sum that
    /// saturates at 30 is `min(first + second, 30)`; a wrap at 40 is `(first + second) mod 40`. Found, the unit is that program (not refuted
    /// any more: it explains what it was told), still stating facts only inside the reach the lessons give it and guessing, with its record,
    /// beyond. Not found, nothing changes. The temporary numbers never enter the library.
    fn repair_regime(&mut self, u: usize) -> bool {
        let unit = &self.units[u];
        if !unit.refuted || !matches!(unit.chosen, Mech::Rule | Mech::Select | Mech::Program) {
            return false;
        }
        let contradicting: Vec<usize> = (0..unit.lessons.len()).filter(|&i| predict(&unit.state, &unit.lessons[i].query, &self.weights).as_deref() != Some(unit.lessons[i].answer.as_str())).collect();
        if contradicting.is_empty() {
            return false;
        }
        let mut extra: Vec<i32> = Vec::new();
        for &i in &contradicting {
            let l = &unit.lessons[i];
            let Some(y) = self.line.value(&l.answer) else { return false };
            let told = [Some(y), predict(&unit.state, &l.query, &self.weights).and_then(|p| self.line.value(&p)).map(|p| (p - y).abs())];
            for v in told.into_iter().flatten() {
                let r = v.round();
                if (v - r).abs() < 0.01 && (3.0..=1000.0).contains(&r) && !extra.contains(&(r as i32)) {
                    extra.push(r as i32);
                }
            }
        }
        extra.truncate(4);
        let mut told: Vec<(String, String)> = contradicting.iter().map(|&i| (unit.lessons[i].query.clone(), unit.lessons[i].answer.clone())).collect();
        told.sort();
        if self.repair_failed.iter().any(|(n, t)| *n == unit.name && *t == told) {
            return false;
        }
        let name = unit.name.clone();
        let Some(mut rule) = ProgramRule::fit_repair_with(&unit.lessons, &self.weights.lib, &extra) else {
            self.repair_failed.retain(|(n, _)| *n != name);
            self.repair_failed.push((name, told));
            return false;
        };
        // (a repair extends a law beyond where it was contradicted; it must not rewrite it where it was working: see `rebuilt_agrees_with_old`)
        if !self.rebuilt_agrees_with_old(u, &rule, &contradicting) {
            self.repair_failed.retain(|(n, _)| *n != name);
            self.repair_failed.push((name, told));
            return false;
        }
        let mut repaired = match &unit.state {
            Fitted::Program(Some(p)) => p.repaired.clone(),
            _ => Vec::new(),
        };
        // (only lessons that CONTRADICTED a prediction are unproven; an answer that merely chose between competing explanations the program declined to
        // choose between is an ordinary test of one of them, like any check)
        for &i in &contradicting {
            if predict(&unit.state, &unit.lessons[i].query, &self.weights).is_some() && !repaired.contains(&unit.lessons[i].query) {
                repaired.push(unit.lessons[i].query.clone());
            }
        }
        rule.repaired = repaired;
        rule.before = match &unit.state {
            Fitted::Program(Some(p)) if p.before.is_some() => p.before.clone(),
            _ => Some(unit.chosen.name().to_string()),
        };
        let before = unit.best_evidence();
        let unit = &mut self.units[u];
        unit.state = Fitted::Program(Some(rule));
        unit.chosen = Mech::Program;
        // (what it knew about this unit before is kept as its reliability: the new program fits every lesson, and was not cross-validated)
        let slot = Mech::ALL.iter().position(|m| *m == Mech::Program).unwrap();
        unit.evidence[slot] = unit.evidence[slot].max(before);
        self.settle_reach(u);
        true
    }

    /// A rebuilt law must agree with the one it replaces at every point the OTHER lessons (all but `contradicting`) vouch for. A program that fits wrongly-told
    /// lessons by a coincidence (a selection repaired into `min(second, first + second div 6)`; a tens digit rebuilt as `b apart ((a + b) mod 10)`, which is the
    /// tens digit until the digits sum past nine) disagrees with the old rule INSIDE the range the other lessons cover, and would state those answers as fact.
    /// (Only where the other lessons vouch for the old rule: the staircase under them, not the whole box.)
    fn rebuilt_agrees_with_old(&self, u: usize, rule: &ProgramRule, contradicting: &[usize]) -> bool {
        let unit = &self.units[u];
        let others: Vec<usize> = (0..unit.lessons.len()).filter(|i| !contradicting.contains(i)).collect();
        let hull = unit.remember.operand_hull(&others, &|w: &str| self.line.value(w), contradicting);
        if let (Some(template), true) = (others.first().map(|&i| unit.lessons[i].query.clone()), !hull.is_empty()) {
            let (a_hi, b_hi) = (hull[0].1.min(30.0) as i32, hull.get(1).map_or(0.0, |h| h.1.min(30.0)) as i32);
            let (a_lo, b_lo) = (hull[0].0.max(0.0) as i32, hull.get(1).map_or(0.0, |h| h.0.max(0.0)) as i32);
            for a in a_lo..=a_hi {
                for b in b_lo..=b_hi {
                    let point: Vec<f32> = if hull.len() > 1 { vec![a as f32, b as f32] } else { vec![a as f32] };
                    if unit.remember.uncovered_by(&[], &|w: &str| self.line.value(w), contradicting, &point).is_some() {
                        continue;
                    }
                    let Some(q) = self.numeric_question(&template, a as f32, b as f32) else { continue };
                    let Some(old) = predict(&unit.state, &q, &self.weights).and_then(|o| self.line.value(&o)) else { continue };
                    let new = rule.expr.eval(a as f32, b as f32);
                    if !new.is_finite() || (new - old).abs() > 0.01 {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// An established exact program meets a fresh exact fit that also explains the few lessons it contradicts (one to three: a wrong answer from a teacher,
    /// or the truth at a point the old law had guessed). Nothing in the lessons says which; the fresh fit is adopted only as a REPAIR: the contradicting
    /// lessons are marked unproven so they do not vouch for their neighbors, and the law it replaced stays a competing explanation (`repair_veto`), so where
    /// the two part ways it is a guess until later lessons separate them. (Refusing the rebuild outright was tried: it breaks the case where the contradiction
    /// is the truth, and the data cannot tell the two cases apart.)
    /// Found by the adaptive exam's wrong-teacher tier: one wrongly told 28 -> 8 swapped the tens digit for a coincidence that stated 19 -> 9 as fact.
    fn rebuild_policy(&self, u: usize, fresh: &Fitted) -> Rebuild {
        let unit = &self.units[u];
        let (Fitted::Program(Some(old)), Fitted::Program(Some(new))) = (&unit.state, fresh) else { return Rebuild::Ordinary };
        if unit.chosen != Mech::Program || old.partial.is_some() || new.partial.is_some() || !old.alts.is_empty() || !old.repaired.is_empty() || unit.facts() < 12 || unit.best_evidence() < mechanisms::reliable() {
            return Rebuild::Ordinary;
        }
        if old.expr.show() == new.expr.show() {
            return Rebuild::Ordinary;
        }
        let contradicting: Vec<usize> = (0..unit.lessons.len()).filter(|&i| predict(&unit.state, &unit.lessons[i].query, &self.weights).as_deref() != Some(unit.lessons[i].answer.as_str())).collect();
        if contradicting.is_empty() || contradicting.len() > 3 {
            return Rebuild::Ordinary;
        }
        Rebuild::Adopt(contradicting.iter().filter(|&&i| predict(&unit.state, &unit.lessons[i].query, &self.weights).is_some()).map(|&i| unit.lessons[i].query.clone()).collect())
    }

    fn apply_rebuild_policy(&self, u: usize, keep_old: bool, mut fresh: Fitted) -> (bool, Fitted) {
        match (keep_old, self.rebuild_policy(u, &fresh)) {
            (true, _) => (true, fresh),
            (false, Rebuild::Adopt(repaired)) => {
                if let Fitted::Program(Some(p)) = &mut fresh {
                    p.repaired = repaired;
                    p.before = Some(self.units[u].chosen.name().to_string());
                    p.rebuilt = true;
                }
                (false, fresh)
            }
            (false, Rebuild::Ordinary) => (false, fresh),
        }
    }

    /// Work out again how to learn a unit, from every lesson it now has.
    fn relearn_unit(&mut self, u: usize) {
        let items = self.units[u].lessons.clone();
        let ev = evidence_thrifty(&items, &self.weights, self.seed);
        let facts = self.units[u].facts();
        let chosen = choose_for_unit(&ev, facts, &items, &self.weights, self.seed);
        let state = mechanisms::fit_for(chosen, &items, &self.weights, self.seed, ev[Mech::ALL.iter().position(|m| *m == chosen).unwrap()]);
        // An established exact program is not thrown away for a few contradicting lessons: one lesson against dozens that agree is an outlier,
        // not a new law (a rule already tolerates up to one in twenty, see `refit_in_place`). It keeps stating facts only where the lessons it
        // explains reach, and a contradiction outside that reach still withholds its guesses there (`settle_reach`), so a genuine change of regime
        // stays safe. More than three, or more than one in twenty, and it is re-decided as before.
        let keep_old = {
            let unit = &self.units[u];
            match (&unit.state, unit.chosen) {
                // (see `refit_in_place`: a repaired program that still explains every lesson stays)
                (Fitted::Program(Some(old)), Mech::Program) if !old.repaired.is_empty() && old.partial.is_none() && items.iter().all(|l| predict(&unit.state, &l.query, &self.weights).as_deref() == Some(l.answer.as_str())) => true,
                (Fitted::Program(Some(old)), Mech::Program) if old.partial.is_none() && (old.alts.is_empty() || !old.repaired.is_empty()) && unit.best_evidence() >= mechanisms::reliable() && (unit.facts() >= 12 || !old.repaired.is_empty()) => {
                    let new_is_exact = matches!(&state, Fitted::Program(Some(p)) if p.partial.is_none());
                    if new_is_exact {
                        false
                    } else {
                        let unexplained = items.iter().filter(|l| predict(&unit.state, &l.query, &self.weights).as_deref() != Some(l.answer.as_str())).count();
                        unexplained <= 3 && unexplained * 20 <= items.len()
                    }
                }
                _ => false,
            }
        };
        let (keep_old, state) = self.apply_rebuild_policy(u, keep_old, state);
        if keep_old {
            self.units[u].remember = Remembered::from_items(&items);
            self.settle_reach(u);
            return;
        }
        let unit = &mut self.units[u];
        unit.evidence = ev;
        unit.chosen = chosen;
        unit.remember = Remembered::from_items(&items);
        unit.state = state;
        if ev[Mech::ALL.iter().position(|m| *m == chosen).unwrap()] >= mechanisms::reliable() {
            mechanisms::grow_library(&mut self.weights.lib, &self.units[u].state, u);
        }
        self.settle_reach(u);
    }

    /// The cheap way to take on a lesson when nothing structural has changed: refit the unit's current mechanism on all its lessons and
    /// check it still explains (nearly) every one. If so there is nothing to re-decide (no cross-validation). Returns false when it does not,
    /// or when there is no settled mechanism to refit: then the unit must be worked out again from scratch.
    fn refit_in_place(&mut self, u: usize) -> bool {
        let unit = &self.units[u];
        let settled_rule = matches!(unit.chosen, Mech::Rule | Mech::Select | Mech::Program) && unit.best_evidence() >= mechanisms::reliable();
        // A unit that is known not to be reliably workable-out (nothing clears the bar) is re-tested only every ten new facts: re-deciding
        // costs a cross-validation of every mechanism, which for a unit that is mostly a rule with a few wrong lessons is 60-260 ms.
        let settled_facts = unit.chosen == Mech::Remember && unit.best_evidence() < mechanisms::reliable() && unit.facts() >= 10 && unit.facts() % 10 != 0;
        if !(settled_rule || settled_facts) {
            return false;
        }
        let items = unit.lessons.clone();
        let chosen = unit.chosen;
        let state = fit(chosen, &items, &self.weights, self.seed);
        // A reliable rule may leave a few lessons unexplained (wrong answers happen); more than one in twenty and it is time to look again.
        if settled_rule {
            let unexplained = items.iter().filter(|l| predict(&state, &l.query, &self.weights).as_deref() != Some(l.answer.as_str())).count();
            if unexplained * 20 > items.len() {
                return false;
            }
        }
        // An established EXACT program is kept when the refit can only offer a partial one because of a few contradicting lessons: one lesson against
        // dozens that agree is an outlier, not a new law. (Without this, a single wrong answer from a teacher turned an exact program into one that
        // only guesses.) It states facts only where the lessons it explains reach, and a contradiction outside that reach still withholds its guesses
        // there (`settle_reach`). More than three contradictions, or one in twenty, and `relearn_unit` re-decides.
        let keep_old = {
            let unit = &self.units[u];
            match (&unit.state, &state) {
                // A repaired program that still explains EVERY lesson (the one just taken in too) is not swapped for a fresh fit: the fresh fit does not know the
                // numbers the repair was given, and the smallest program that fits can be a coincidence (`second + (first min 18)` agrees with two probes that
                // `min(first + second, 30)` also gets right), whose lessons would then vouch for what lies below them.
                (Fitted::Program(Some(old)), _) if !old.repaired.is_empty() && old.partial.is_none() && items.iter().all(|l| predict(&unit.state, &l.query, &self.weights).as_deref() == Some(l.answer.as_str())) => true,
                (Fitted::Program(Some(old)), Fitted::Program(Some(new))) if old.partial.is_none() && old.alts.is_empty() && new.partial.is_some() && unit.facts() >= 12 => {
                    let unexplained = items.iter().filter(|l| predict(&unit.state, &l.query, &self.weights).as_deref() != Some(l.answer.as_str())).count();
                    unexplained <= 3 && unexplained * 20 <= items.len()
                }
                _ => false,
            }
        };
        let (keep_old, state) = self.apply_rebuild_policy(u, keep_old, state);
        let unit = &mut self.units[u];
        unit.remember = Remembered::from_items(&items);
        if !keep_old {
            unit.state = if chosen == Mech::Remember { Fitted::Remember(unit.remember.clone()) } else { state };
        }
        self.settle_reach(u);
        true
    }

    /// What would it like to be told? For each unit of numbers that it can only memorize because the lessons leave several explanations
    /// equally good, the questions (in the unit's own phrasing and number style) whose answers would settle which is meant. Nothing is
    /// asked about units that have no explanation at all: an arbitrary table has no hypotheses to tell apart.
    pub fn wonder(&self) -> Vec<Wonder> {
        self.wonder_about(None)
    }

    /// As `wonder`, about one unit only (the search for explanations is the costly part).
    pub fn wonder_about(&self, only: Option<&str>) -> Vec<Wonder> {
        let mut out = Vec::new();
        for unit in &self.units {
            let open = match &unit.state {
                Fitted::Program(Some(p)) => !p.alts.is_empty(),
                _ => unit.chosen == Mech::Remember,
            };
            if !open || unit.facts() < 5 || only.map_or(false, |n| n != unit.name) {
                continue;
            }
            let own = match &unit.state {
                Fitted::Program(Some(p)) if !p.repaired.is_empty() => p.own_ambiguity(&unit.lessons),
                _ => None,
            };
            let Some(amb) = own.or_else(|| ProgramRule::ambiguity(&unit.lessons, &self.weights.lib)) else { continue };
            let Some(template) = unit.lessons.iter().find(|l| tokenize(&l.query).iter().filter(|w| self.line.value(w).is_some()).count() == amb.arity) else { continue };
            for &(a, b) in amb.inputs.iter().take(2) {
                let Some(question) = self.numeric_question(&template.query, a, b) else { continue };
                let says: Vec<String> = amb.hypotheses.iter().map(|e| format!("{} says {}", e.show(), e.eval(a, b))).collect();
                out.push(Wonder {
                    unit: unit.name.clone(),
                    question,
                    why: format!("what I was taught fits {} explanations that part ways here: {}", amb.hypotheses.len(), says.join("; ")),
                });
            }
        }
        out
    }

    /// Has the library changed since the units were last looked at? (Only then can a unit left open be settled by a new piece.)
    fn library_changed(&self) -> bool {
        self.weights.lib.to_lines() != self.library_seen
    }

    /// Look again at the numeric units that were left open (a program with competing explanations, or plain memory because nothing fit) now
    /// that its vocabulary may have grown: a ten, or a program learned since, can settle what could not be settled before. Repeats while the
    /// library keeps changing (a unit settled this way can be the piece another needed).
    pub fn reconsider_open(&mut self) {
        mechanisms::set_reliable(self.bar);
        self.reconsider(false)
    }

    /// As `reconsider_open`, and also the program units that look settled: they may have been fitted before the vocabulary that would have
    /// given them a competing explanation existed (a digit sum explained by "mod 9" before it had a tens digit and a units digit).
    pub fn review_programs(&mut self) {
        mechanisms::set_reliable(self.bar);
        self.reconsider(true)
    }

    fn reconsider(&mut self, programs_too: bool) {
        let changed = timed("review:library_changed", || self.library_changed());
        if !changed {
            return;
        }
        for _ in 0..3 {
            timed("review:round", || ());
            let before = self.weights.lib.to_lines();
            for u in 0..self.units.len() {
                let open = match &self.units[u].state {
                    // (a partial program is provisional by nature: a piece the library has since gained may let an exact program fit)
                    Fitted::Program(Some(p)) => programs_too || !p.alts.is_empty() || p.partial.is_some(),
                    _ => self.units[u].chosen == Mech::Remember && self.units[u].facts() >= 5,
                };
                // only units of numbers: a unit of facts has no program to find
                let numeric = self.units[u].lessons.iter().any(|l| tokenize(&l.query).iter().any(|w| self.line.value(w).is_some()))
                    && self.units[u].lessons.iter().all(|l| self.line.value(&l.answer).is_some());
                // a program that looked settled is only re-fitted if, with today's vocabulary, something else fits the lessons as well
                // (one search that ends as soon as it has found its fits, far cheaper than fitting the unit again)
                let settled_program = matches!(&self.units[u].state, Fitted::Program(Some(p)) if p.alts.is_empty() && p.partial.is_none());
                if open && numeric && (!settled_program || ProgramRule::ambiguity(&self.units[u].lessons, &self.weights.lib).is_some()) {
                    // a program that was only ever a rebuild around a few contradicting lessons is not sticky when the vocabulary has grown and
                    // something else fits as well (it was never built from numbers a contradiction named)
                    if let Fitted::Program(Some(p)) = &mut self.units[u].state {
                        if p.rebuilt {
                            p.rebuilt = false;
                            p.repaired = Vec::new();
                            p.before = None;
                        }
                    }
                    self.relearn_unit(u);
                }
            }
            if self.weights.lib.to_lines() == before {
                break;
            }
        }
        self.library_seen = self.weights.lib.to_lines();
    }

    /// The unit's own phrasing (`template`, one of its lessons) with its numbers replaced by `a` and `b`; in digits if the lesson used digits,
    /// else in words (only possible up to ten).
    fn numeric_question(&self, template: &str, a: f32, b: f32) -> Option<String> {
        let digits = crate::quantity::uses_digits(template);
        let mut numbers = vec![a, b].into_iter();
        let mut words = Vec::new();
        for t in tokenize(template) {
            if self.line.value(&t).is_some() {
                let v = numbers.next().unwrap_or(0.0);
                if digits {
                    words.push((v as i64).to_string());
                } else {
                    words.push(crate::data::NUM.get(v as usize).filter(|_| v <= 10.0)?.to_string());
                }
            } else {
                words.push(t);
            }
        }
        Some(words.join(" "))
    }

    /// Where would it like to check itself? For a unit of numbers that works by a program, a rule or a selection (and has no competing
    /// explanation): (1) for a program, the numbers nobody taught inside the taught range (a gap) and the first one past it; (2) for all of
    /// them, the edge of what it was taught: each operand pushed to about twice its largest taught value, and the order of the operands swapped
    /// if it never varied. These are the places where a rule that is right on everything taught can still be wrong (a sum that saturates, one
    /// that wraps, a "first operand" that was only ever shown when it was bigger) and nothing in the lessons says so. `limit` questions per unit.
    /// Each confirmed answer is kept, and extends the range the unit may answer in when it is `careful`.
    pub fn check_myself(&self, limit: usize) -> Vec<Wonder> {
        let mut out = Vec::new();
        for (ui, unit) in self.units.iter().enumerate() {
            // a repaired program with competing explanations: ask where they part ways (the nearest places first), so that one answer can choose
            if let Fitted::Program(Some(p)) = &unit.state {
                if !p.repaired.is_empty() && !p.alts.is_empty() && p.partial.is_none() {
                    if let (Some(amb), Some(template)) = (p.own_ambiguity(&unit.lessons), unit.lessons.iter().find(|l| tokenize(&l.query).iter().filter(|w| self.line.value(w).is_some()).count() == p.arity())) {
                        for &(a, b) in amb.inputs.iter().take(limit.min(2)) {
                            let Some(question) = self.numeric_question(&template.query, a, b) else { continue };
                            let says: Vec<String> = amb.hypotheses.iter().map(|e| format!("{} says {}", e.show(), e.eval(a, b))).collect();
                            out.push(Wonder { unit: unit.name.clone(), question, why: format!("I rebuilt this unit's rule after a correction and {} explanations fit what I was told but part ways here: {}", amb.hypotheses.len(), says.join("; ")) });
                        }
                    }
                    continue;
                }
            }
            let (arity_hint, program_text) = match &unit.state {
                Fitted::Program(Some(p)) if p.alts.is_empty() && p.partial.is_none() => (Some(p.arity()), Some(p.expr.show())),
                Fitted::Rule(_) | Fitted::Select(_) if unit.best_evidence() >= mechanisms::reliable() => (None, None),
                _ => continue,
            };
            if unit.facts() < 5 {
                continue;
            }
            let numbers_of = |q: &str| -> Vec<f32> { tokenize(q).iter().filter_map(|w| self.line.value(w)).collect() };
            let Some(template) = unit.lessons.iter().find(|l| {
                let n = numbers_of(&l.query).len();
                (1..=2).contains(&n) && arity_hint.map_or(true, |a| a == n)
            }) else { continue };
            let arity = numbers_of(&template.query).len();
            let taught: std::collections::HashSet<(i64, i64)> = unit
                .lessons
                .iter()
                .filter(|l| numbers_of(&l.query).len() == arity)
                .map(|l| {
                    let n = numbers_of(&l.query);
                    (n[0] as i64, *n.get(1).unwrap_or(&0.0) as i64)
                })
                .collect();
            let hi_a = taught.iter().map(|t| t.0).max().unwrap_or(0);
            let lo_a = taught.iter().map(|t| t.0).min().unwrap_or(0);
            let hi_b = taught.iter().map(|t| t.1).max().unwrap_or(0);
            let lo_b = taught.iter().map(|t| t.1).min().unwrap_or(0);
            let mut candidates: Vec<(i64, i64, &str)> = Vec::new();
            // gaps, for a program (its explanation is a guess about every number nobody taught)
            if program_text.is_some() {
                if arity == 1 {
                    candidates.extend((lo_a..=hi_a).filter(|a| !taught.contains(&(*a, 0))).map(|a| (a, 0, "inside the range I was taught")));
                } else {
                    let mut gaps: Vec<(i64, i64)> = (lo_a..=hi_a).flat_map(|a| (0..=hi_b).map(move |b| (a, b))).filter(|c| !taught.contains(c)).collect();
                    gaps.sort_by_key(|c| c.0 + c.1);
                    candidates.extend(gaps.into_iter().map(|(a, b)| (a, b, "inside the range I was taught")));
                }
            }
            // the edge: each operand pushed well past its largest taught value
            let gap_a = (hi_a - lo_a).max(3);
            if arity == 1 {
                candidates.push((hi_a + gap_a, 0, "past the largest number I was taught"));
            } else {
                let gap_b = (hi_b - lo_b).max(3);
                candidates.push((hi_a + gap_a, hi_b, "past the largest first number I was taught"));
                candidates.push((hi_a, hi_b + gap_b, "past the largest second number I was taught"));
                // the corner of the box its reach describes: both numbers large at once (a sum that wraps shows itself only here)
                candidates.push((hi_a + gap_a, hi_b + gap_b, "past the largest of both numbers I was taught, at once"));
                // the order of the operands: if the first was always the bigger (or the smaller), the opposite has never been seen
                let always_first_bigger = taught.iter().all(|t| t.0 >= t.1);
                let always_first_smaller = taught.iter().all(|t| t.0 <= t.1);
                if (always_first_bigger || always_first_smaller) && taught.iter().any(|t| t.0 != t.1) {
                    if let Some(&(a, b)) = taught.iter().filter(|t| t.0 != t.1).max_by_key(|t| t.0 + t.1) {
                        candidates.push((b, a, "the two numbers the other way round, which I was never shown"));
                    }
                }
            }
            // the floor: zero, if nothing at or near it was taught (below the smallest taught number is as unchecked as above the largest)
            if arity == 1 {
                if lo_a > 0 {
                    candidates.push((0, 0, "below the smallest number I was taught"));
                }
            } else {
                if lo_a > 0 {
                    candidates.push((0, hi_b, "below the smallest first number I was taught"));
                }
                if lo_b > 0 {
                    candidates.push((hi_a, 0, "below the smallest second number I was taught"));
                }
            }
            // At most two gap questions go first: a program has hundreds of untaught pairs inside its range, and they must not crowd out the edge.
            let (gaps, mut edges): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|c| c.2 == "inside the range I was taught");
            // cheap, decisive edges first (the floor, the swapped order), then the far ones
            edges.sort_by_key(|c| if c.2.starts_with("below") { 0 } else if c.2.starts_with("the two numbers") { 1 } else { 2 });
            let mut candidates: Vec<(i64, i64, &str)> = gaps.iter().take(2).copied().chain(edges).chain(gaps.iter().skip(2).copied()).collect();
            candidates.retain(|c| c.0 <= 9999 && c.1 <= 9999 && !(arity == 1 && c.1 != 0));
            let by = match (&unit.state, &program_text) {
                (_, Some(text)) => format!("I explain this unit with {text}"),
                (Fitted::Select(_), _) => "I explain this unit by picking the bigger or smaller".to_string(),
                _ => "I explain this unit with a rule I fitted".to_string(),
            };
            let mut asked = 0;
            for &(a0, b0, where_) in candidates.iter() {
                if asked >= limit {
                    break;
                }
                // A probe pushed past the edge is asked as far out as it can be asked: if the far one cannot be put in words or its answer cannot be
                // said (10 + 5 on a number line that ends at ten), step back toward the edge (5 + 5, 6 + 0) instead of giving up on the edge.
                let (push_a, push_b) = match where_ {
                    "past the largest number I was taught" | "past the largest first number I was taught" => (true, false),
                    "past the largest second number I was taught" => (false, true),
                    "past the largest of both numbers I was taught, at once" => (true, true),
                    _ => (false, false),
                };
                let back = if push_a || push_b { ((a0 - hi_a).max(b0 - hi_b)).max(1) } else { 1 };
                let mut found: Option<String> = None;
                for k in 0..back {
                    let a = if push_a { (a0 - k).max(hi_a + 1) } else { a0 };
                    let b = if push_b { (b0 - k).max(hi_b + 1) } else { b0 };
                    let Some(question) = self.numeric_question(&template.query, a as f32, b as f32) else { continue };
                    // Never ask for an answer it could not read back: if what it expects is not a number it can represent (more than four digits),
                    // the teacher's answer would be one it cannot learn from, and the question would only damage what it knows.
                    match self.unit_predicts(ui, &question) {
                        Some(expected) if expected.parse::<f64>().map_or(true, |v| v <= 9999.0) => {}
                        // no prediction (it cannot say that number) or an unreadable one: try a nearer probe
                        _ => continue,
                    }
                    found = Some(question);
                    break;
                }
                let Some(question) = found else { continue };
                asked += 1;
                out.push(Wonder { unit: unit.name.clone(), question, why: format!("{by}, and it fits everything I was taught, but nobody has told me about a number {where_}") });
            }
            // Where the probes above run out (the number line ends, so the far ones cannot be said), probe the frontier of what CAN be asked: the
            // farthest askable numbers that would still be refused as fact (outside the range, or not covered by anything confirmed), none of them
            // below another. Careful mode only: that is where "not covered" means something.
            // (it always gets at least two places: a program unit's in-range "gap" questions would otherwise use up the whole budget every round)
            let frontier_places = limit.saturating_sub(asked).max(2);
            if self.careful {
                let top = hi_a.max(hi_b);
                let m = (2 * top).max(top + 3).min(30);
                let mut pts: Vec<(i64, i64, String)> = Vec::new();
                for a in 0..=m {
                    for b in 0..=(if arity == 2 { m } else { 0 }) {
                        let Some(q) = self.numeric_question(&template.query, a as f32, b as f32) else { continue };
                        match self.unit_predicts(ui, &q) {
                            Some(expected) if expected.parse::<f64>().map_or(true, |v| v <= 9999.0) => {}
                            _ => continue,
                        }
                        if self.beyond_reach(ui, &tokenize(&q)).is_some() && !out.iter().any(|w: &Wonder| w.unit == unit.name && w.question == q) {
                            pts.push((a, b, q));
                        }
                    }
                }
                let mut maximal: Vec<&(i64, i64, String)> =
                    pts.iter().filter(|p| !pts.iter().any(|q| (q.0, q.1) != (p.0, p.1) && q.0 >= p.0 && q.1 >= p.1)).collect();
                maximal.sort_by_key(|p| std::cmp::Reverse((p.0 + p.1, p.0)));
                for p in maximal.into_iter().take(frontier_places) {
                    out.push(Wonder {
                        unit: unit.name.clone(),
                        question: p.2.clone(),
                        why: format!("{by}, and it fits everything I was taught, but nobody has told me about the farthest number I can be asked about that I have not checked"),
                    });
                }
            }
        }
        out
    }

    /// Take the answer to a question it wondered about, and think again about how that unit is learned, now: a unit that can only memorize is
    /// otherwise looked at again only every tenth new fact.
    pub fn learn_wondered(&mut self, query: &str, answer: &str, unit: &str) -> Result<Learned, String> {
        mechanisms::set_reliable(self.bar);
        if self.skeptical {
            if let Some(u) = self.units.iter().position(|x| x.name == unit) {
                if let Some(held) = timed("wondered:weigh_against_rule", || self.weigh_against_rule(u, query, answer)) {
                    return Ok(held);
                }
            }
        }
        let learned = timed("wondered:learn", || self.learn(query, answer, Some(unit)))?;
        if let Some(u) = self.units.iter().position(|x| x.name == unit) {
            // An answer it asked for is kept even if it already predicted it: a confirmed check is what extends the range it may answer in.
            if !learned.surprised {
                let key = lesson_key(query);
                if self.units[u].lessons.iter().all(|l| lesson_key(&l.query) != key) {
                    let fact = self.units.iter().flat_map(|x| x.lessons.iter()).map(|l| l.fact).max().map_or(0, |m| m + 1);
                    let template = self.units[u].lessons.len() % 4;
                    self.units[u].lessons.push(Item { unit: u, fact, template, query: query.to_string(), answer: answer.to_string() });
                    self.add_to_tables(u, query);
                }
            }
            // Think the unit through again, now: unless it is a settled exact rule, selection or program (nothing to decide: a surprising answer was already
            // taken in by `learn`, a confirming one only extends where it may state facts), which looks again only every `SETTLED_RETHINK_EVERY` lessons.
            // (Re-deciding is a cross-validation of every mechanism: it was most of the energy of a stream of told answers, 1.8 J each.)
            // (A unit that can only memorize, and is known not to be workable out, follows the rule `refit_in_place` has for it: look again every ten facts.)
            let facts = self.units[u].facts();
            // (and a settled exact PROGRAM that still explains every lesson has nothing to be re-decided at all: the 25-lesson look found the same program every time)
            let skip = (self.settled_exact(u) && facts % SETTLED_RETHINK_EVERY != 0) || (self.memorize_only_known(u) && facts % 10 != 0) || self.settled_program_explaining_all(u);
            // (and not when `learn` has just re-decided this very unit from these very lessons and library: that would only repeat it)
            let repeat = matches!(&self.just_relearned, Some((ju, n, lib)) if *ju == u && *n == self.units[u].lessons.len() && *lib == self.weights.lib.to_lines());
            if !skip && !repeat {
                let t0 = std::time::Instant::now();
                let before_state = format!("{:?} ev {:.0} partial {}", self.units[u].chosen, self.units[u].best_evidence(), matches!(&self.units[u].state, Fitted::Program(Some(p)) if p.partial.is_some()));
                timed("wondered:relearn_unit", || self.relearn_unit(u));
                if std::env::var("RELEARN_DEBUG").is_ok() {
                    eprintln!("RELEARN {} facts {} surprised {} {} -> {:?} in {:.2}s", self.units[u].name, facts, learned.surprised, before_state, self.units[u].chosen, t0.elapsed().as_secs_f64());
                }
            } else if skip && !learned.surprised {
                let items = self.units[u].lessons.clone();
                self.units[u].remember = Remembered::from_items(&items);
                if self.units[u].chosen == Mech::Remember {
                    self.units[u].state = Fitted::Remember(self.units[u].remember.clone());
                }
                self.settle_reach(u);
            }
            timed("wondered:repair_regime", || self.repair_regime(u));
        }
        // a change in vocabulary can give a settled-looking program a rival: ask each one, cheaply, whether it is still the only explanation
        timed("wondered:review_programs", || self.review_programs());
        timed("wondered:recalibrate_route", || self.maybe_recalibrate_route());
        Ok(learned)
    }

    /// Does the unit's current explanation say every one of its lessons exactly? (For a settled exact program that is the case where nothing about how to learn
    /// the unit can have changed: the program is still the smallest that fits, since a smaller one fitting all lessons would have fitted the earlier ones.)
    fn explains_every_lesson(&self, u: usize) -> bool {
        let unit = &self.units[u];
        unit.lessons.iter().all(|l| predict(&unit.state, &l.query, &self.weights).as_deref() == Some(l.answer.as_str()))
    }

    /// An exact program (no rivals, not partial) that explains every lesson including the one just taken in.
    fn settled_program_explaining_all(&self, u: usize) -> bool {
        let unit = &self.units[u];
        unit.chosen == Mech::Program && self.settled_exact(u) && matches!(&unit.state, Fitted::Program(Some(_))) && self.explains_every_lesson(u)
    }

    /// Is unit `u` a rule, selection or exact program with no competing explanation, reliable on the facts held back? (Nothing left to decide about how to learn it.)
    fn settled_exact(&self, u: usize) -> bool {
        let unit = &self.units[u];
        matches!(unit.chosen, Mech::Rule | Mech::Select | Mech::Program)
            && unit.best_evidence() >= mechanisms::reliable()
            && !matches!(&unit.state, Fitted::Program(Some(p)) if p.partial.is_some() || !p.alts.is_empty())
            && !matches!(&unit.state, Fitted::Program(None) | Fitted::Remember(_) | Fitted::Network(_))
    }

    /// Is unit `u` one that only memorizes, because nothing it tried reaches the bar (and it has enough facts to have been tested)?
    fn memorize_only_known(&self, u: usize) -> bool {
        let unit = &self.units[u];
        unit.chosen == Mech::Remember && unit.best_evidence() < mechanisms::reliable() && unit.facts() >= 10 && matches!(unit.state, Fitted::Remember(_))
    }

    /// `learn_wondered` for a teacher who repeats themselves: if the answer is held for a second opinion, say it again (so a contradiction that
    /// is meant is believed, as when a person insists). For simulated always-right teachers and for tests; an interactive caller should instead
    /// ask the person again, using `Learned.disputed`.
    pub fn learn_wondered_sure(&mut self, query: &str, answer: &str, unit: &str) -> Result<Learned, String> {
        let first = self.learn_wondered(query, answer, unit)?;
        if first.disputed { self.learn_wondered(query, answer, unit) } else { Ok(first) }
    }

    /// Teach it one more lesson, after school. With `unit` it joins that unit (or opens one of that name); without, it joins the
    /// unit the question resembles, if it recognizes a kind at all. Then the unit's way of learning is worked out again.
    pub fn learn(&mut self, query: &str, answer: &str, unit: Option<&str>) -> Result<Learned, String> {
        mechanisms::set_reliable(self.bar);
        self.just_relearned = None;
        let tokens = tokenize(query);
        if tokens.is_empty() {
            return Err("nothing to learn: the question is empty".into());
        }
        if let Some(bad) = unreadable_number(query) {
            return Err(format!("I only read plain whole numbers, not '{bad}': a lesson with a sign, decimal, fraction or percent would be stored wrongly"));
        }
        let (u, opened) = match unit {
            Some(name) => match self.units.iter().position(|x| x.name == name) {
                Some(u) => (u, false),
                None => {
                    // A new kind of lesson. It starts as plain memory until it has enough facts to be tested.
                    self.units.push(SchoolUnit {
                        name: name.to_string(),
                        chosen: Mech::Remember,
                        evidence: [0.0; 5],
                        lessons: Vec::new(),
                        remember: Remembered::from_items(&[]),
                        state: Fitted::Remember(Remembered::from_items(&[])),
                        exceptions: Vec::new(),
                        refuted: false,
                        signatures: Vec::new(),
                    });
                    (self.units.len() - 1, true)
                }
            },
            None => {
                let unmet: Vec<&String> = tokens.iter().filter(|w| !self.knows(w)).collect();
                if !unmet.is_empty() {
                    return Err(format!(
                        "I haven't met {}, so I can't tell which kind of lesson this is. Say where it belongs with --unit NAME (an existing unit to add a phrasing, or a new name to open one)",
                        unmet.iter().map(|w| format!("'{w}'")).collect::<Vec<_>>().join(", ")
                    ));
                }
                let (u, score) = self.route(query, &mut Ledger::default());
                if score < self.route_floor {
                    return Err("this doesn't look like any kind of lesson I know. Give it a name with --unit NAME to open a new unit".into());
                }
                (u, false)
            }
        };
        let was = if opened { None } else { Some(self.units[u].chosen) };
        // Surprise-gated: if the unit can already work this out, there is nothing to learn and nothing to keep.
        // (A confirmed GUESS is not nothing to learn: it is what extends the range the unit may state as fact.)
        let beyond = if self.careful && !opened { self.beyond_reach(u, &tokens).map(|(_, stretch)| stretch) } else { None };
        let was_a_guess = beyond.is_some();
        // it would have offered a guess only from a numeric rule that no lesson had refuted
        let guessed_at = beyond.filter(|_| !self.units[u].refuted && matches!(self.units[u].chosen, Mech::Rule | Mech::Select | Mech::Program));
        // Its own record as a guesser: it would have guessed here (the unit has a prediction), and now it is told.
        if let (Some(stretch), Some(guess)) = (guessed_at, if guessed_at.is_some() { self.unit_predicts(u, query) } else { None }) {
            let slot = &mut self.guess_record[stretch_bucket(stretch)];
            if guess == answer { slot.0 += 1 } else { slot.1 += 1 }
        }
        if !opened
            && !was_a_guess
            && self.units[u].lessons.iter().all(|l| lesson_key(&l.query) != lesson_key(query))
            && self.unit_predicts(u, query).as_deref() == Some(answer)
        {
            let unit = &self.units[u];
            return Ok(Learned { unit: unit.name.clone(), opened: false, corrected: false, was, now: unit.chosen, held_back: unit.best_evidence(), facts: unit.facts(), surprised: false, disputed: false });
        }
        let key = lesson_key(query);
        let corrected;
        if let Some(existing) = self.units[u].lessons.iter_mut().find(|l| lesson_key(&l.query) == key) {
            // the words are unchanged, so the weights and the routing table are too
            existing.answer = answer.to_string();
            corrected = true;
        } else {
            let fact = self.units.iter().flat_map(|x| x.lessons.iter()).map(|l| l.fact).max().map_or(0, |m| m + 1);
            let template = self.units[u].lessons.len() % 4;
            self.units[u].lessons.push(Item { unit: u, fact, template, query: query.to_string(), answer: answer.to_string() });
            self.add_to_tables(u, query);
            corrected = false;
        }
        // A settled law that this lesson contradicts is given the chance to be repaired BEFORE the unit is re-decided: a small unit re-decided on one
        // contradiction falls back to memory (one lesson in sixteen is more than a rule tolerates), and then there is no law left to repair.
        let was_settled = !opened && matches!(self.units[u].chosen, Mech::Rule | Mech::Select | Mech::Program) && self.units[u].best_evidence() >= mechanisms::reliable();
        // (a repaired program that declines because its competing explanations disagree is also being told something: which of them is right)
        let undecided_repair = matches!(&self.units[u].state, Fitted::Program(Some(p)) if !p.repaired.is_empty() && !p.alts.is_empty());
        let contradicts = was_settled
            && match predict(&self.units[u].state, query, &self.weights) {
                Some(p) => p != answer,
                None => undecided_repair,
            };
        let repaired_now = if contradicts {
            self.units[u].remember = Remembered::from_items(&self.units[u].lessons);
            self.settle_reach(u);
            self.repair_regime(u)
        } else {
            false
        };
        // A unit that only guesses (its program fits most lessons, or is not reliable enough to state facts) costs seconds to refit, since the tolerant
        // search is expensive: a lesson updates what it remembers and how questions are routed at once, and the program is looked at again every
        // `GUESSER_REFIT_EVERY` lessons.
        let guesser = self.cheap_guessers && matches!(&self.units[u].state, Fitted::Program(Some(p)) if p.partial.is_some());
        if guesser && !repaired_now && self.units[u].facts() % GUESSER_REFIT_EVERY != 0 {
            let items = self.units[u].lessons.clone();
            self.units[u].remember = Remembered::from_items(&items);
        } else if !repaired_now && self.settled_program_explaining_all(u) {
            // a settled exact program that already says every lesson, this one too: refitting it would find the same program (see `explains_every_lesson`); the
            // lesson only extends what it remembers and where it may state facts
            let items = self.units[u].lessons.clone();
            self.units[u].remember = Remembered::from_items(&items);
            self.settle_reach(u);
        } else if !repaired_now {
            let mut relearned = false;
            if !timed("learn:refit_in_place", || self.refit_in_place(u)) {
                let t0 = std::time::Instant::now();
                let (was_chosen, was_ev) = (self.units[u].chosen, self.units[u].best_evidence());
                timed("learn:relearn_unit", || self.relearn_unit(u));
                if std::env::var("RELEARN_DEBUG").is_ok() {
                    eprintln!("LEARN-RELEARN {} lessons {} facts {} {:?} ev {:.0} -> {:?} in {:.2}s", self.units[u].name, self.units[u].lessons.len(), self.units[u].facts(), was_chosen, was_ev, self.units[u].chosen, t0.elapsed().as_secs_f64());
                }
                relearned = true;
            }
            let repaired = timed("learn:repair_regime", || self.repair_regime(u));
            if relearned && !repaired {
                self.just_relearned = Some((u, self.units[u].lessons.len(), self.weights.lib.to_lines()));
            }
        }
        let unit = &self.units[u];
        let learned = Learned { unit: unit.name.clone(), opened, corrected, was, now: unit.chosen, held_back: unit.best_evidence(), facts: unit.facts(), surprised: true, disputed: false };
        self.maybe_recalibrate_route();
        Ok(learned)
    }

    fn renumber(&mut self) {
        for (i, u) in self.units.iter_mut().enumerate() {
            for l in &mut u.lessons {
                l.unit = i;
            }
        }
    }

    /// (For tests) are the incrementally kept weights and routing table exactly what a rebuild from every lesson gives?
    #[cfg(test)]
    fn tables_equal_a_rebuild(&self) -> bool {
        let mut weights = Weights::from_lists(
            self.units.iter().enumerate().flat_map(|(u, unit)| unit.lessons.iter().map(move |l| (u, tokenize(&l.query)))).collect(),
        );
        weights.lib = self.weights.lib.clone();
        weights == self.weights && Routing::build(&self.units, &self.line) == self.routing
    }

    fn word_keyed(&self, u: usize) -> bool {
        matches!(self.units[u].chosen, Mech::Rule | Mech::Select) && self.units[u].best_evidence() >= mechanisms::reliable()
    }

    /// Look again at how the lessons are grouped. A unit that is no longer fully explained (its newer lessons stopped following one
    /// rule) is taken apart if its lessons separate into groups that each ARE explained; two units that were taught separately but
    /// are right for each other's lessons under one rule, or a rule per word, become one.
    pub fn reorganize(&mut self) -> Reorg {
        let report = self.reorganize_inner();
        self.settle_all_reach();
        self.recalibrate_route();
        report
    }

    fn reorganize_inner(&mut self) -> Reorg {
        // the noise in its lessons may have changed since it was taught: measure again, and use the bar that goes with it
        self.bar = mechanisms::bar_for_noise(mechanisms::estimate_noise(&self.units.iter().map(|u| u.lessons.clone()).collect::<Vec<_>>()));
        mechanisms::set_reliable(self.bar);
        let mut report = Reorg { merged: Vec::new(), split: Vec::new() };

        // 1. Split what has stopped following one rule.
        let candidates: Vec<String> = self
            .units
            .iter()
            .filter(|u| {
                let numeric = u.lessons.iter().filter(|l| tokenize(&l.query).iter().any(|w| self.line.value(w).is_some())).count();
                u.facts() >= 10 && u.best_evidence() < 0.95 && numeric * 10 >= u.lessons.len() * 8
            })
            .map(|u| u.name.clone())
            .collect();
        for name in candidates {
            let Some(u) = self.units.iter().position(|x| x.name == name) else { continue };
            let pairs: Vec<(String, String)> = self.units[u].lessons.iter().map(|l| (l.query.clone(), l.answer.clone())).collect();
            let d = crate::discover::discover(&pairs, self.seed);
            if d.names.len() < 2 {
                continue;
            }
            let mut moved: Vec<usize> = Vec::new(); // indices into this unit's lessons that leave
            let mut targets: Vec<(usize, Vec<Item>)> = Vec::new();
            // The biggest group stays where it is: the unit is its, and the others leave.
            let biggest = (0..d.names.len()).max_by_key(|g| d.items.iter().filter(|it| it.unit == *g).count()).unwrap_or(0);
            for g in 0..d.names.len() {
                if g == biggest {
                    continue;
                }
                let group: Vec<(&Item, usize)> = d.items.iter().zip(&d.origin).filter(|(it, _)| it.unit == g).map(|(it, o)| (it, *o)).collect();
                let items: Vec<Item> = group.iter().map(|(it, _)| (*it).clone()).collect();
                let facts = items.iter().map(|i| i.fact).collect::<std::collections::HashSet<_>>().len();
                if facts < 5 {
                    continue;
                }
                let best = mechanisms::cv_accuracy(&items, &self.weights, self.seed, &[1, 2, 3]).into_iter().fold(0.0, f64::max);
                if best < 0.9 {
                    continue;
                }
                // The group must end up under a real mechanism, not memory (a group named after a filler word, "is", once left a unit of
                // plain memory and took good lessons out of a working rule), and what stays behind must be reliable too: a split has to
                // make things better.
                let group_ev = mechanisms::evidence_thrifty(&items, &self.weights, self.seed);
                if choose_mechanism(&group_ev, facts) == Mech::Remember {
                    continue;
                }
                let moved_here: std::collections::HashSet<usize> = group.iter().map(|(_, o)| *o).collect();
                let remainder: Vec<Item> = self.units[u].lessons.iter().enumerate().filter(|(i, _)| !moved.contains(i) && !moved_here.contains(i)).map(|(_, l)| l.clone()).collect();
                let rest_ev = mechanisms::evidence_thrifty(&remainder, &self.weights, self.seed);
                if choose_mechanism(&rest_ev, remainder.iter().map(|l| l.fact).collect::<std::collections::HashSet<_>>().len()) == Mech::Remember {
                    continue;
                }
                // Where do they go? To a unit that builds exactly this expression, if there is one; else a unit of their own.
                let own = ProgramRule::fit_unique(&items).map(|p| p.expr.to_text());
                let same = own.as_ref().and_then(|t| {
                    self.units.iter().enumerate().position(|(i, x)| i != u && matches!(&x.state, Fitted::Program(Some(p)) if &p.expr.to_text() == t))
                });
                let target = match same {
                    Some(t) => t,
                    None => {
                        let mut unique = d.names[g].clone();
                        let mut n = 2;
                        while self.units.iter().any(|x| x.name == unique) {
                            unique = format!("{}{n}", d.names[g]);
                            n += 1;
                        }
                        self.units.push(SchoolUnit {
                            name: unique,
                            chosen: Mech::Remember,
                            evidence: [0.0; 5],
                            lessons: Vec::new(),
                            remember: Remembered::from_items(&[]),
                            state: Fitted::Remember(Remembered::from_items(&[])),
                            exceptions: Vec::new(),
                            refuted: false,
                            signatures: Vec::new(),
                        });
                        self.units.len() - 1
                    }
                };
                moved.extend(group.iter().map(|(_, o)| *o));
                targets.push((target, items));
            }
            if moved.is_empty() {
                continue;
            }
            let fact_base = all_items(&self.units).iter().map(|l| l.fact).max().map_or(0, |m| m + 1);
            let mut keep: Vec<Item> = Vec::new();
            for (i, l) in std::mem::take(&mut self.units[u].lessons).into_iter().enumerate() {
                if !moved.contains(&i) {
                    keep.push(l);
                }
            }
            self.units[u].lessons = keep;
            for (target, items) in &targets {
                for it in items {
                    self.units[*target].lessons.push(Item { unit: *target, fact: it.fact + fact_base, template: 0, query: it.query.clone(), answer: it.answer.clone() });
                }
                report.split.push((name.clone(), self.units[*target].name.clone(), items.len()));
            }
            self.rebuild_tables();
            let mut touched: Vec<usize> = targets.iter().map(|t| t.0).chain(std::iter::once(u)).collect();
            touched.sort();
            touched.dedup();
            for t in touched {
                self.relearn_unit(t);
            }
        }

        // 2. Merge what turns out to be one. Rule units (and selection units) that one rule fits are tested as a whole set, not pair by
        //    pair; only when the set does not fit as one is it halved. (A pair scan that starts over after every merge cost O(units^2)
        //    cross-validations, and every merge re-validated the whole growing family: 100 s at 200 units.)
        loop {
            let mut groups: Vec<Group> = Vec::new();
            for mech in [Mech::Rule, Mech::Select] {
                let cands: Vec<usize> = (0..self.units.len()).filter(|&u| self.word_keyed(u) && self.units[u].chosen == mech).collect();
                if cands.len() >= 2 {
                    groups.extend(self.mergeable_groups(&cands).into_iter().filter(|g| g.units.len() >= 2));
                }
            }
            if groups.is_empty() {
                break;
            }
            let mut slots: Vec<Option<SchoolUnit>> = std::mem::take(&mut self.units).into_iter().map(Some).collect();
            let mut kept: Vec<(String, (usize, f64))> = Vec::new();
            for g in &groups {
                let mut keep = slots[g.units[0]].take().unwrap();
                for &b in &g.units[1..] {
                    let gone = slots[b].take().unwrap();
                    report.merged.push((keep.name.clone(), gone.name.clone()));
                    keep.lessons.extend(gone.lessons);
                    let parts: Vec<&str> = keep.name.split('/').collect();
                    keep.name = if parts.len() >= 3 { format!("{}/{}+", parts[0], parts[1]) } else { format!("{}/{}", keep.name, gone.name) };
                }
                kept.push((keep.name.clone(), g.fit.expect("a merged group was tested as a whole")));
                slots[g.units[0]] = Some(keep);
            }
            self.units = slots.into_iter().flatten().collect();
            self.renumber();
            self.rebuild_tables();
            // Settle each merged unit from the evidence that justified the merge. (Working it out again from scratch, cross-validating
            // every mechanism on the whole family, cost 6.9 s for 564 lessons to rediscover what the merge test had just shown.)
            for (name, (mech, accuracy)) in kept {
                if let Some(u) = self.units.iter().position(|x| x.name == name) {
                    let items = self.units[u].lessons.clone();
                    let state = fit(Mech::ALL[mech], &items, &self.weights, self.seed);
                    let unit = &mut self.units[u];
                    unit.chosen = Mech::ALL[mech];
                    unit.evidence = [0.0; 5];
                    unit.evidence[mech] = accuracy;
                    unit.remember = Remembered::from_items(&items);
                    unit.state = state;
                    self.settle_reach(u);
                }
            }
        }
        self.renumber();
        self.rebuild_tables();
        self.reconsider_open();
        if !report.merged.is_empty() || !report.split.is_empty() {
            self.recalibrate_floor();
        }
        report
    }

    /// Split a set of units into groups that each follow one rule together (a group of one stays alone). Tries the whole set first; if one
    /// rule does not fit them all, halves it, sorts out each half, then tries to fold the second half's groups into the first's.
    fn mergeable_groups(&self, set: &[usize]) -> Vec<Group> {
        if set.len() < 2 {
            return vec![Group { units: set.to_vec(), fit: None }];
        }
        if let Some(fit) = self.fit_as_one(set) {
            return vec![Group { units: set.to_vec(), fit: Some(fit) }];
        }
        let (left, right) = set.split_at(set.len() / 2);
        let mut groups = self.mergeable_groups(left);
        for g in self.mergeable_groups(right) {
            let joined = groups.iter().enumerate().find_map(|(i, h)| {
                let both: Vec<usize> = h.units.iter().chain(&g.units).copied().collect();
                self.fit_as_one(&both).map(|fit| (i, fit))
            });
            match joined {
                Some((i, fit)) => {
                    groups[i].units.extend(g.units);
                    groups[i].fit = Some(fit);
                }
                None => groups.push(g),
            }
        }
        groups
    }

    /// Does one rule (or selection) fit every one of these units on facts held back? If so: which mechanism, and how accurate.
    fn fit_as_one(&self, units: &[usize]) -> Option<(usize, f64)> {
        let parts: Vec<&[Item]> = units.iter().map(|&u| self.units[u].lessons.as_slice()).collect();
        crate::discover::can_merge_many(&parts, &self.weights, self.seed)
    }

    /// Set the recognition floor from the table as it is now: low enough that every lesson it holds is recognized, with room below
    /// for phrasings it has not seen (those are less decisive than taught ones).
    fn recalibrate_floor(&mut self) {
        let mut margins: Vec<f32> = all_items_ref(&self.units).into_iter().map(|l| self.route(&l.query, &mut Ledger::default()).1).collect();
        margins.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if let Some(m) = margins.first() {
            self.route_floor = m - 0.6 * m.abs();
        }
    }

    /// What this one unit would answer, ignoring whether the question was sent to it.
    fn unit_predicts(&self, u: usize, q: &str) -> Option<String> {
        let unit = &self.units[u];
        match &unit.state {
            Fitted::Network(_) => None,
            Fitted::Remember(_) => unit.remember.recall_counted(q, &self.weights, &mut Ledger::default(), true),
            other => predict(other, q, &self.weights),
        }
    }

    // ---- going on learning without being told where things belong ----

    /// Teach it one more lesson without saying which unit it belongs to. If it recognizes a kind of lesson it joins that unit.
    /// If not, it goes to the pen; and when enough pen lessons gather to be a kind of their own, a unit opens by itself.
    pub fn learn_anywhere(&mut self, query: &str, answer: &str) -> Result<Outcome, String> {
        mechanisms::set_reliable(self.bar);
        let tokens = tokenize(query);
        if tokens.is_empty() {
            return Err("nothing to learn: the question is empty".into());
        }
        if let Some(bad) = unreadable_number(query) {
            return Err(format!("I only read plain whole numbers, not '{bad}': a lesson with a sign, decimal, fraction or percent would be stored wrongly"));
        }
        let unmet = tokens.iter().any(|w| !self.knows(w));
        let recognized = !unmet && self.route(query, &mut Ledger::default()).1 >= self.route_floor;
        if recognized {
            return self.learn(query, answer, None).map(Outcome::Placed);
        }
        match self.pen.iter_mut().find(|(q, _)| lesson_key(q) == lesson_key(query)) {
            Some(existing) => existing.1 = answer.to_string(),
            None => self.pen.push((query.to_string(), answer.to_string())),
        }
        let report = self.process_pen();
        Ok(Outcome::Penned { pen: self.pen.len(), report })
    }

    /// Look at the lessons waiting in the pen. A group of at least five different facts that follows one rule becomes a unit (or
    /// joins the unit that already builds exactly the same expression). Anything smaller stays, with a note if an existing unit
    /// already gets it right, for the teacher to confirm.
    pub fn process_pen(&mut self) -> PenReport {
        let report = self.process_pen_inner();
        self.settle_all_reach();
        report
    }

    fn process_pen_inner(&mut self) -> PenReport {
        mechanisms::set_reliable(self.bar);
        let mut report = PenReport { opened: Vec::new(), joined: Vec::new(), suggestions: Vec::new(), left: 0 };
        let mut used = vec![false; self.pen.len()];
        let mut changed: Vec<usize> = Vec::new();
        if self.pen.len() >= 6 {
            let d = crate::discover::discover(&self.pen, self.seed);
            let fact_base = all_items(&self.units).iter().map(|l| l.fact).max().map_or(0, |m| m + 1);
            for (u, name) in d.names.iter().enumerate() {
                let group: Vec<(&Item, usize)> = d.items.iter().zip(&d.origin).filter(|(it, _)| it.unit == u).map(|(it, o)| (it, *o)).collect();
                let facts = group.iter().map(|(it, _)| it.fact).collect::<std::collections::HashSet<_>>().len();
                if facts < 5 {
                    continue;
                }
                let items: Vec<Item> = group.iter().map(|(it, _)| (*it).clone()).collect();
                let own = ProgramRule::fit_unique(&items).map(|p| p.expr.to_text());
                let same = own.as_ref().and_then(|t| self.units.iter().position(|x| matches!(&x.state, Fitted::Program(Some(p)) if &p.expr.to_text() == t)));
                let target = match same {
                    Some(e) => e,
                    None => {
                        // Name it after the words that are new to the baby: they are what makes this a kind of its own.
                        let new_words: Vec<String> = {
                            let mut v: Vec<(usize, String)> = group
                                .iter()
                                .flat_map(|(it, _)| tokenize(&it.query))
                                .collect::<std::collections::BTreeSet<_>>()
                                .into_iter()
                                .filter(|t| !self.knows(t))
                                .map(|t| (group.iter().filter(|(it, _)| tokenize(&it.query).contains(&t)).count(), t))
                                .collect();
                            v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                            v.into_iter().take(2).map(|x| x.1).collect()
                        };
                        let name = if new_words.is_empty() { name.clone() } else { new_words.join("/") };
                        let mut unique = name.clone();
                        let mut n = 2;
                        while self.units.iter().any(|x| x.name == unique) {
                            unique = format!("{name}{n}");
                            n += 1;
                        }
                        self.units.push(SchoolUnit {
                            name: unique,
                            chosen: Mech::Remember,
                            evidence: [0.0; 5],
                            lessons: Vec::new(),
                            remember: Remembered::from_items(&[]),
                            state: Fitted::Remember(Remembered::from_items(&[])),
                            exceptions: Vec::new(),
                            refuted: false,
                            signatures: Vec::new(),
                        });
                        self.units.len() - 1
                    }
                };
                for (it, o) in &group {
                    self.units[target].lessons.push(Item { unit: target, fact: it.fact + fact_base, template: 0, query: it.query.clone(), answer: it.answer.clone() });
                    used[*o] = true;
                }
                if same.is_some() {
                    report.joined.push((self.units[target].name.clone(), group.len()));
                } else {
                    report.opened.push((self.units[target].name.clone(), group.len()));
                }
                changed.push(target);
                // New facts need fresh ids that cannot collide with the next batch.
                let _ = fact_base;
            }
        }
        let leftover: Vec<(String, String)> =
            self.pen.iter().enumerate().filter(|(i, _)| !used[*i]).map(|(_, l)| l.clone()).collect();
        self.pen = leftover;
        if !changed.is_empty() {
            self.rebuild_tables();
            changed.sort();
            changed.dedup();
            for u in changed {
                self.relearn_unit(u);
            }
        }
        // For what is left: does an existing unit already work these out? Say so, and let the teacher confirm.
        if self.pen.len() >= 3 {
            for unit in &self.units {
                let right = self
                    .pen
                    .iter()
                    .filter(|(q, a)| {
                        let got = match &unit.state {
                            Fitted::Rule(r) => r.answer_ignoring(q, &|_| true).filter(|(_, d)| *d < 0.25).map(|x| x.0),
                            Fitted::Select(s) => s.predict(q, &|_| true),
                            Fitted::Program(Some(p)) => p.predict(q, &|_| true),
                            _ => None,
                        };
                        got.as_deref() == Some(a.as_str())
                    })
                    .count();
                // A built expression is only a fair suggestion if the waiting lessons admit exactly that expression and nothing else.
                let program_ok = match &unit.state {
                    Fitted::Program(Some(p)) => {
                        let items: Vec<Item> = self.pen.iter().enumerate().map(|(i, (q, a))| Item { unit: 0, fact: i, template: 0, query: q.clone(), answer: a.clone() }).collect();
                        ProgramRule::fit_unique(&items).map_or(false, |f| f.expr.to_text() == p.expr.to_text())
                    }
                    _ => true,
                };
                if program_ok && right * 20 >= self.pen.len() * 19 && unit.best_evidence() >= 0.95 {
                    report.suggestions.push(format!(
                        "all {} lessons waiting look like another way of asking unit '{}' (what it already knows gets every one right): confirm with --unit {}",
                        self.pen.len(), unit.name, unit.name
                    ));
                }
            }
        }
        report.left = self.pen.len();
        report
    }

    /// Forget what is already explained. A unit that learned a rule, a selection or an expression does not need every lesson it was
    /// taught: it keeps a small support set (the facts the fitted unit could not have got right without, and every word it needs
    /// to understand). A unit of facts keeps two phrasings of each. What it knows does not change.
    pub fn compact(&mut self) -> Compacted {
        let report = self.compact_inner();
        self.settle_all_reach();
        report
    }

    fn compact_inner(&mut self) -> Compacted {
        mechanisms::set_reliable(self.bar);
        let before: usize = self.units.iter().map(|u| u.lessons.len()).sum();
        let mut removed: Vec<Item> = Vec::new();
        for u in 0..self.units.len() {
            let mask = self.keep_mask(u);
            let lessons = std::mem::take(&mut self.units[u].lessons);
            for (l, keep) in lessons.into_iter().zip(mask) {
                if keep {
                    self.units[u].lessons.push(l);
                } else {
                    removed.push(l);
                }
            }
        }
        // The promise: it must still answer EVERY lesson it was ever taught, kept or let go. Check, and put back whatever it can no
        // longer answer (typically a phrasing the table or the memory only recognized because of that lesson), until it can.
        loop {
            for u in 0..self.units.len() {
                if self.units[u].chosen == Mech::Remember {
                    let r = Remembered::from_items(&self.units[u].lessons);
                    self.units[u].remember = r.clone();
                    self.units[u].state = Fitted::Remember(r);
                }
            }
            self.rebuild_tables();
            // Recognition floor: low enough that EVERYTHING it was taught, kept or let go, is recognized.
            let mut margins: Vec<f32> =
                all_items_ref(&self.units).into_iter().chain(removed.iter()).map(|l| self.route(&l.query, &mut Ledger::default()).1).collect();
            margins.sort_by(|a, b| a.partial_cmp(b).unwrap());
            // Unseen phrasings are less decisive than taught ones, so leave room below the least decisive lesson.
            self.route_floor = margins[0] - 0.6 * margins[0].abs();
            let (back, still): (Vec<Item>, Vec<Item>) =
                std::mem::take(&mut removed).into_iter().partition(|l| self.ask(&l.query).answer.as_deref() != Some(l.answer.as_str()));
            removed = still;
            if back.is_empty() {
                break;
            }
            for l in back {
                let u = l.unit;
                self.units[u].lessons.push(l);
            }
        }
        let after: usize = self.units.iter().map(|u| u.lessons.len()).sum();
        Compacted { before, after }
    }

    fn keep_mask(&self, u: usize) -> Vec<bool> {
        let unit = &self.units[u];
        let n = unit.lessons.len();
        match unit.chosen {
            Mech::Rule | Mech::Select | Mech::Program if unit.best_evidence() >= 0.95 => {
                let mut facts: Vec<usize> = unit.lessons.iter().map(|l| l.fact).collect();
                facts.sort();
                facts.dedup();
                crate::rng::Rng::new(self.seed.wrapping_add(u as u64 * 17 + 3)).shuffle(&mut facts);
                let mut chosen: Vec<usize> = facts.iter().take(6).cloned().collect();
                loop {
                    let items: Vec<Item> = unit.lessons.iter().filter(|l| chosen.contains(&l.fact)).cloned().collect();
                    let fitted = fit(unit.chosen, &items, &self.weights, self.seed);
                    let wrong = facts.iter().find(|f| {
                        !chosen.contains(f)
                            && unit.lessons.iter().filter(|l| l.fact == **f).any(|l| predict(&fitted, &l.query, &self.weights).as_deref() != Some(l.answer.as_str()))
                    });
                    match wrong {
                        Some(f) => chosen.push(*f),
                        None => break,
                    }
                }
                unit.lessons.iter().map(|l| chosen.contains(&l.fact)).collect()
            }
            Mech::Remember => {
                let mut seen: HashMap<usize, usize> = HashMap::new();
                unit.lessons
                    .iter()
                    .map(|l| {
                        let c = seen.entry(l.fact).or_default();
                        *c += 1;
                        *c <= 2
                    })
                    .collect()
            }
            _ => vec![true; n],
        }
    }

    // ---- saving and loading ----

    fn state_text(&self, i: usize, path: &str) -> std::io::Result<String> {
        let mut out = String::new();
        match &self.units[i].state {
            Fitted::Remember(_) => writeln!(out, "state remember").unwrap(),
            Fitted::Rule(r) => {
                let ops = r.all_ops();
                writeln!(out, "state rule {}", ops.len()).unwrap();
                for (w, o) in ops {
                    writeln!(out, "o {w} {} {} {}", o[0], o[1], o[2]).unwrap();
                }
            }
            Fitted::Select(s) => {
                let sc = s.scores();
                writeln!(out, "state select {}", sc.len()).unwrap();
                for (w, x) in sc {
                    writeln!(out, "c {w} {x}").unwrap();
                }
            }
            Fitted::Program(Some(p)) => {
                writeln!(out, "state program {}", p.arity()).unwrap();
                writeln!(out, "expr {}", p.expr.to_text()).unwrap();
                writeln!(out, "vocab {}", p.vocab().join(" ")).unwrap();
                writeln!(out, "alts {}", p.alts.len()).unwrap();
                for e in &p.alts {
                    writeln!(out, "alt {}", e.to_text()).unwrap();
                }
                if let Some(share) = p.partial {
                    writeln!(out, "partial {share}").unwrap();
                }
                if !p.repaired.is_empty() {
                    writeln!(out, "repaired {} {}", p.repaired.len(), p.before.as_deref().unwrap_or("-")).unwrap();
                    for q in &p.repaired {
                        writeln!(out, "rep {q}").unwrap();
                    }
                }
            }
            Fitted::Program(None) => writeln!(out, "state none").unwrap(),
            Fitted::Network(m) => {
                let file = format!("{path}.unit{i}.model");
                m.save(&file)?;
                writeln!(out, "state network {file}").unwrap();
            }
        }
        Ok(out)
    }

    fn lessons_text(&self, i: usize) -> String {
        let u = &self.units[i];
        let mut out = String::new();
        writeln!(out, "lessons {}", u.lessons.len()).unwrap();
        for l in &u.lessons {
            writeln!(out, "l {} {} {}\t{}", l.fact, l.template, l.answer, l.query).unwrap();
        }
        out
    }

    /// For each unit: (name, mechanism, lessons kept, bytes of lessons, bytes of what it learned from them).
    pub fn storage(&self) -> Vec<(String, Mech, usize, usize, usize)> {
        (0..self.units.len())
            .map(|i| {
                let state = self.state_text(i, "/dev/null/none").map(|t| t.len()).unwrap_or(0);
                (self.units[i].name.clone(), self.units[i].chosen, self.units[i].lessons.len(), self.lessons_text(i).len(), state)
            })
            .collect()
    }

    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let mut out = String::new();
        writeln!(out, "SCHOOL2").unwrap();
        writeln!(out, "seed {}", self.seed).unwrap();
        writeln!(out, "route_floor {}", self.route_floor).unwrap();
        out.push_str(&self.weights.lib.to_lines());
        writeln!(out, "bar {}", self.bar).unwrap();
        if self.route_ratio > 0.0 {
            writeln!(out, "route_calibration {} {}", self.route_base, self.route_ratio).unwrap();
        }
        // careful is the default, so only its absence is written (a file from before careful mode loads as careful)
        if !self.careful {
            writeln!(out, "careful 0").unwrap();
        }
        if self.guess_record.iter().any(|g| *g != (0, 0)) {
            let nums: Vec<String> = self.guess_record.iter().flat_map(|g| [g.0.to_string(), g.1.to_string()]).collect();
            writeln!(out, "guessed {}", nums.join(" ")).unwrap();
        }
        if !self.disputed.is_empty() {
            writeln!(out, "disputed {}", self.disputed.len()).unwrap();
            for (unit, query, answer) in &self.disputed {
                writeln!(out, "x {unit}\t{answer}\t{query}").unwrap();
            }
        }
        if let Some(sel) = &self.selector {
            out.push_str(&sel.to_lines());
        }
        if self.guess_scale != [1.0, 1.0] {
            writeln!(out, "guessscale {} {}", self.guess_scale[0], self.guess_scale[1]).unwrap();
        }
        writeln!(out, "pen {}", self.pen.len()).unwrap();
        for (q, a) in &self.pen {
            writeln!(out, "p {a}\t{q}").unwrap();
        }
        writeln!(out, "units {}", self.units.len()).unwrap();
        for (i, u) in self.units.iter().enumerate() {
            let e = u.evidence;
            writeln!(out, "unit {i} {} {} {} {} {} {} {}", u.name, u.chosen.name(), e[0], e[1], e[2], e[3], e[4]).unwrap();
            out.push_str(&self.lessons_text(i));
            out.push_str(&self.state_text(i, path)?);
        }
        writeln!(out, "end").unwrap();
        if let Some(dir) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, out)
    }

    pub fn load(path: &str) -> Result<School, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
        let mut it = text.lines().peekable();
        if it.next() != Some("SCHOOL2") {
            return Err(format!("{path}: not a SCHOOL2 file (teach it again with: baby school)"));
        }
        let field = |l: Option<&str>, key: &str| -> Result<String, String> {
            l.and_then(|l| l.strip_prefix(&format!("{key} "))).map(String::from).ok_or_else(|| format!("{path}: expected '{key}'"))
        };
        let seed: u64 = field(it.next(), "seed")?.parse().map_err(|e| format!("{e}"))?;
        let route_floor: f32 = field(it.next(), "route_floor")?.parse().map_err(|e| format!("{e}"))?;
        // what it had learned to build with (a file written before there was a library has none)
        let library = Library::from_lines(&mut it).unwrap_or_default();
        let bar: f64 = match it.peek().and_then(|l| l.strip_prefix("bar ")) {
            Some(b) => {
                let b = b.trim().parse().map_err(|e| format!("{e}"))?;
                it.next();
                b
            }
            None => 0.9,
        };
        let (route_base, route_ratio) = match it.peek().and_then(|l| l.strip_prefix("route_calibration ")) {
            Some(rest) => {
                let v: Vec<f32> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                it.next();
                if v.len() == 2 { (v[0], v[1]) } else { (route_floor, 0.0) }
            }
            None => (route_floor, 0.0),
        };
        // "careful 1" was written by files made when permissive was the default; both it and a missing line mean careful
        let careful = match it.peek().map(|l| l.trim()) {
            Some("careful 0") => {
                it.next();
                false
            }
            Some("careful 1") => {
                it.next();
                true
            }
            _ => true,
        };
        let mut guess_record = [(0u32, 0u32); 5];
        if let Some(line) = it.peek().and_then(|l| l.strip_prefix("guessed ")) {
            let nums: Vec<u32> = line.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if nums.len() == 10 {
                for (i, g) in guess_record.iter_mut().enumerate() {
                    *g = (nums[2 * i], nums[2 * i + 1]);
                }
            }
            it.next();
        }
        let mut disputed: Vec<(String, String, String)> = Vec::new();
        if let Some(n) = it.peek().and_then(|l| l.strip_prefix("disputed ")).and_then(|n| n.trim().parse::<usize>().ok()) {
            it.next();
            for _ in 0..n {
                let l = it.next().ok_or("disputed line")?.strip_prefix("x ").ok_or("bad disputed line")?;
                let mut p = l.splitn(3, '\t');
                // (the file says: unit, answer, question)
                let (unit, answer, query) = (p.next().unwrap_or("").to_string(), p.next().unwrap_or("").to_string(), p.next().unwrap_or("").to_string());
                disputed.push((unit, query, answer));
            }
        }
        let selector = crate::selector::Selector::from_lines(&mut it);
        let mut guess_scale = [1.0f32, 1.0];
        if let Some(line) = it.peek().and_then(|l| l.strip_prefix("guessscale ")) {
            let v: Vec<f32> = line.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if v.len() == 2 {
                guess_scale = [v[0], v[1]];
            }
            it.next();
        }
        let pen_n: usize = field(it.next(), "pen")?.parse().map_err(|e| format!("{e}"))?;
        let mut pen = Vec::new();
        for _ in 0..pen_n {
            let l = it.next().ok_or("pen line")?.strip_prefix("p ").ok_or("bad pen line")?;
            let (a, q) = l.split_once('\t').ok_or("bad pen line")?;
            pen.push((q.to_string(), a.to_string()));
        }
        let count: usize = field(it.next(), "units")?.parse().map_err(|e| format!("{e}"))?;
        let mut units = Vec::new();
        for u in 0..count {
            let head: Vec<&str> = it.next().ok_or("missing unit")?.split_whitespace().collect();
            if head.len() != 9 || head[0] != "unit" {
                return Err(format!("{path}: bad unit line"));
            }
            let chosen = Mech::ALL.iter().find(|m| m.name() == head[3]).copied().ok_or("unknown mechanism")?;
            let mut evidence = [0.0f64; 5];
            for k in 0..5 {
                evidence[k] = head[4 + k].parse().map_err(|e| format!("{e}"))?;
            }
            let n: usize = field(it.next(), "lessons")?.parse().map_err(|e| format!("{e}"))?;
            let mut lessons = Vec::new();
            for _ in 0..n {
                let l = it.next().ok_or("lesson line")?.strip_prefix("l ").ok_or("bad lesson line")?;
                let (meta, query) = l.split_once('\t').ok_or("bad lesson line")?;
                let m: Vec<&str> = meta.split_whitespace().collect();
                lessons.push(Item {
                    unit: u,
                    fact: m[0].parse().map_err(|e| format!("{e}"))?,
                    template: m[1].parse().map_err(|e| format!("{e}"))?,
                    answer: m[2].to_string(),
                    query: query.to_string(),
                });
            }
            let remember = Remembered::from_items(&lessons);
            let state_line: Vec<&str> = it.next().ok_or("state")?.split_whitespace().collect();
            let state = match state_line.as_slice() {
                ["state", "remember"] => Fitted::Remember(remember.clone()),
                ["state", "none"] => Fitted::Program(None),
                ["state", "rule", n] => {
                    let n: usize = n.parse().map_err(|e| format!("{e}"))?;
                    let mut ops = Vec::new();
                    for _ in 0..n {
                        let f: Vec<&str> = it.next().ok_or("rule line")?.split_whitespace().collect();
                        let p = |i: usize| f[i].parse::<f32>().map_err(|e| format!("{e}"));
                        ops.push((f[1].to_string(), [p(2)?, p(3)?, p(4)?]));
                    }
                    Fitted::Rule(Quantity::from_ops(ops))
                }
                ["state", "select", n] => {
                    let n: usize = n.parse().map_err(|e| format!("{e}"))?;
                    let mut sc = Vec::new();
                    for _ in 0..n {
                        let f: Vec<&str> = it.next().ok_or("select line")?.split_whitespace().collect();
                        sc.push((f[1].to_string(), f[2].parse::<f32>().map_err(|e| format!("{e}"))?));
                    }
                    Fitted::Select(SelectRule::from_scores(sc))
                }
                ["state", "program", arity] => {
                    let arity: usize = arity.parse().map_err(|e| format!("{e}"))?;
                    let expr = Expr::from_text(it.next().ok_or("expr")?.strip_prefix("expr ").ok_or("bad expr")?).ok_or("bad expression")?;
                    let vocab: Vec<String> = it.next().ok_or("vocab")?.split_whitespace().skip(1).map(String::from).collect();
                    let mut rule = ProgramRule::rebuild(expr, arity, vocab);
                    // the other explanations that fit (a file taught before they were kept has none)
                    let ahead = it.clone().next().and_then(|l| l.strip_prefix("alts ")).and_then(|n| n.trim().parse::<usize>().ok());
                    if let Some(n) = ahead {
                        it.next();
                        for _ in 0..n {
                            let t = it.next().ok_or("alt line")?.strip_prefix("alt ").ok_or("bad alt line")?;
                            rule.alts.push(Expr::from_text(t).ok_or("bad alternative")?);
                        }
                    }
                    if let Some(share) = it.clone().next().and_then(|l| l.strip_prefix("partial ")).and_then(|x| x.trim().parse::<f32>().ok()) {
                        it.next();
                        rule.partial = Some(share);
                    }
                    if let Some((n, before)) = it.clone().next().and_then(|l| l.strip_prefix("repaired ")).and_then(|l| {
                        let mut parts = l.split_whitespace();
                        Some((parts.next()?.parse::<usize>().ok()?, parts.next().filter(|b| *b != "-").map(String::from)))
                    }) {
                        rule.before = before;
                        it.next();
                        for _ in 0..n {
                            rule.repaired.push(it.next().ok_or("rep line")?.strip_prefix("rep ").ok_or("bad rep line")?.to_string());
                        }
                    }
                    Fitted::Program(Some(rule))
                }
                ["state", "network", file] => Fitted::Network(Box::new(Model::load(file)?)),
                other => return Err(format!("{path}: bad state line {other:?}")),
            };
            units.push(SchoolUnit { name: head[2].to_string(), chosen, evidence, lessons, remember, state, exceptions: Vec::new(), refuted: false, signatures: Vec::new() });
        }
        let line = NumberLine::from_counting(&counting_lessons());
        let mut weights = Weights::from(&all_items(&units));
        weights.lib = library;
        let routing = Routing::build(&units, &line);
        let library_seen = weights.lib.to_lines();
        let mut school = School { units, weights, routing, line, route_floor, route_base, route_ratio, calibrated_lessons: 0, seed, library_seen, bar, careful, skeptical: true, disputed, guess_record, repair_failed: Vec::new(), selector, guess_scale, told_since_retrain: 0, cheap_guessers: false, old_law: Default::default(), just_relearned: None, selector_examples: Vec::new(), route_precision: Vec::new(), pen };
        school.calibrated_lessons = school.total_lessons();
        school.settle_all_reach();
        Ok(school)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds;

    /// The capability tests ask what the baby can work out, so they run permissive; the careful default has its own tests.
    fn trained() -> School {
        let mut s = School::train(&kinds::world(1), 1, true);
        s.set_careful(false);
        s
    }

    #[test]
    fn it_answers_what_it_can_work_out_and_says_how() {
        let s = trained();
        let r = s.ask("what is seven times one");
        assert_eq!(r.answer.as_deref(), Some("seven"));
        assert!(r.how.contains("expression I built"), "{}", r.how);
        assert_eq!(s.ask("nine minus four").answer.as_deref(), Some("five"));
        assert_eq!(s.ask("which is bigger seven or nine").answer.as_deref(), Some("nine"));
        assert_eq!(s.ask("how far apart are two and ten").answer.as_deref(), Some("eight"));
        assert_eq!(s.ask("capital of peru").answer.as_deref(), Some("lima"));
        assert_eq!(s.ask("what is half of ten").answer.as_deref(), Some("five"));
    }

    #[test]
    fn digits_are_numbers_the_answer_follows_the_question_and_the_old_refusals_still_hold() {
        let s = trained();
        let said = |q: &str| s.ask(q).answer;
        // taught in words only; digits are numbers too, and the answer is in the form the question used
        assert_eq!(said("what is 3 plus 4").as_deref(), Some("7"));
        assert_eq!(said("what is 17 plus 25").as_deref(), Some("42"));
        assert_eq!(said("what is 3 plus four").as_deref(), Some("7"), "any digit makes the answer digits");
        assert_eq!(said("what is three plus four").as_deref(), Some("seven"));
        assert_eq!(said("what is 12 times 12").as_deref(), Some("144"), "a built expression reaches past the words");
        assert_eq!(said("which is bigger 7 or 12").as_deref(), Some("12"));
        assert_eq!(said("how far apart are 3 and 40").as_deref(), Some("37"));
        assert_eq!(said("what is double 21").as_deref(), Some("42"));
        assert_eq!(said("what comes after 99").as_deref(), Some("100"));
        assert_eq!(said("what is 3 plus 4?").as_deref(), Some("7"), "ordinary punctuation is fine");
        assert_eq!(said("what is 3 plus 4.").as_deref(), Some("7"));
        let mut t = trained();
        assert!(t.learn("what is -3 plus 5", "2", Some("sum")).is_err(), "a lesson with a sign is refused, not stored without it");
        assert!(t.learn_anywhere("what is 1.5 plus 1", "2").is_err());
        // the same refusals as in words
        for q in [
            "three minus 5", "what is 2 minus 5", "half of 9", "what is 3 times 4 times 5", "what is 99999 plus 1", "what is 3 plus", "what is plus 4",
            "what is the code for 7", "color of 5", "capital of 7", "what is 3.5 plus 2", "what is -3 plus 5", "what is 1,000 plus 5", "what is 1/2 plus 1", "what is 50% plus 5", "what is 3 plus +4", "what is $5 plus 5",
        ] {
            let r = s.ask(q);
            assert!(r.answer.is_none(), "answered '{q}' with {:?} ({})", r.answer, r.how);
        }
    }

    #[test]
    fn nonsense_unrelated_and_out_of_range_questions_are_refused_never_answered() {
        let s = trained();
        for q in [
            "capital of banana", "color of france", "what does the sky say", "opposite of paris", "what is the capital of three",
            "double banana", "what is hot times cold", "how far apart are paris and rome", "which is bigger paris or rome",
            "what comes after banana", "half of cat", "what is the meaning of life", "who won the world cup", "tell me a joke", "hello",
            "what time is it", "what is five times five", "what is ten plus ten", "three minus five", "what comes after ten",
            "half of seven",
        ] {
            let r = s.ask(q);
            assert!(r.answer.is_none(), "answered '{q}' with {:?} ({})", r.answer, r.how);
        }
    }

    #[test]
    fn in_a_memorize_only_unit_it_never_answers_a_fact_it_was_not_taught() {
        let s = trained();
        let nums = crate::data::NUM;
        let (mut right, mut refused) = (0, 0);
        for n in 0..11 {
            let r = s.ask(&format!("what is the code for {}", nums[n]));
            match r.answer.as_deref() {
                Some(a) => {
                    assert_eq!(a, nums[kinds::CODE[n]], "bluffed on the code for {}", nums[n]);
                    right += 1;
                }
                None => refused += 1,
            }
        }
        assert!(right >= 7 && refused >= 1, "right {right}, refused {refused}");
    }

    #[test]
    fn forgetting_what_is_explained_halves_the_lessons_and_keeps_every_promise() {
        let w = kinds::world(1);
        let mut s = School::train(&w, 1, true);
        s.set_careful(false);
        let c = s.compact();
        assert!(c.after * 10 < c.before * 6, "kept {} of {}", c.after, c.before);
        // It still answers every lesson it was ever taught, right, and never wrongly on anything else it is asked.
        for it in &w.train {
            assert_eq!(s.ask(&it.query).answer.as_deref(), Some(it.answer.as_str()), "forgot how to answer '{}'", it.query);
        }
        for it in w.reworded.iter().chain(&w.new_items).chain(&w.beyond) {
            if let Some(a) = s.ask(&it.query).answer {
                assert_eq!(a, it.answer, "answered '{}' wrongly after forgetting", it.query);
            }
        }
        for q in ["capital of banana", "three minus five", "what is hot times cold", "tell me a joke", "what comes after ten", "half of cat"] {
            assert!(s.ask(q).answer.is_none(), "answered '{q}' after forgetting");
        }
    }

    #[test]
    fn lessons_that_fit_nothing_wait_in_a_pen_and_become_a_unit_when_enough_gather() {
        let mut s = trained();
        let units = s.units.len();
        for (n, a) in [("zero", "ten"), ("one", "nine"), ("two", "eight"), ("three", "seven"), ("four", "six"), ("five", "five")] {
            let out = s.learn_anywhere(&format!("what is {n} less than ten"), a).unwrap();
            assert!(matches!(out, Outcome::Penned { .. }), "a kind of lesson it has not seen must wait, not be forced into a unit");
        }
        assert_eq!(s.units.len(), units + 1, "six lessons of one new kind should have opened a unit by themselves");
        assert!(s.pen.is_empty());
        let opened = s.units.last().unwrap();
        assert_ne!(opened.chosen, Mech::Remember, "it should have found something that generalizes: {:?}", opened.chosen);
        assert_eq!(s.ask("what is eight less than ten").answer.as_deref(), Some("two"));
    }

    #[test]
    fn a_lesson_it_can_already_work_out_costs_nothing_and_is_not_kept() {
        let mut s = trained();
        let lessons_before: usize = s.units.iter().map(|u| u.lessons.len()).sum();
        // The sums rule was taught up to five; "seven plus two" it can already work out.
        let r = s.learn("seven plus two", "nine", Some("sum")).unwrap();
        assert!(!r.surprised, "it already knew this");
        let r = s.learn("what is two multiplied by three", "six", Some("times")).unwrap();
        assert!(r.surprised, "but 'multiplied by' is new");
        let lessons_after: usize = s.units.iter().map(|u| u.lessons.len()).sum();
        assert_eq!(lessons_after, lessons_before + 1, "only the surprising lesson was kept");
    }

    #[test]
    fn a_unit_that_stopped_following_one_rule_is_taken_apart() {
        let mut s = trained();
        // Six multiplication lessons are put under 'sum' by mistake.
        for (q, a) in [
            ("what is two times three", "six"), ("one times five", "five"), ("what is two times five", "ten"),
            ("three times three", "nine"), ("what is zero times four", "zero"), ("four times two", "eight"),
        ] {
            s.learn(q, a, Some("sum")).unwrap();
        }
        let sum = s.units.iter().find(|u| u.name == "sum").unwrap();
        assert!(sum.best_evidence() < 0.95, "pollution should show: {}", sum.best_evidence());
        let times_before = s.units.iter().find(|u| u.name == "times").unwrap().lessons.len();
        let report = s.reorganize();
        assert!(!report.split.is_empty(), "the multiplication lessons should have been taken out");
        let sum = s.units.iter().find(|u| u.name.starts_with("sum")).unwrap();
        assert!(sum.best_evidence() >= 0.95, "sum is itself again: {}", sum.best_evidence());
        let times = s.units.iter().find(|u| u.name == "times").unwrap();
        assert!(times.lessons.len() >= times_before + 6, "they went to the unit that builds the same expression");
        assert_eq!(s.ask("what is two times four").answer.as_deref(), Some("eight"));
        assert_eq!(s.ask("seven plus two").answer.as_deref(), Some("nine"));
    }

    /// A unit of five lessons is judged on a fair share of the folds, so whether it can join a rule family does not depend on the seed.
    /// (Before the folds were stratified by part it joined on only 17 of 24 seeds: a random split left it two lessons to learn from.)
    #[test]
    fn whether_a_small_unit_can_join_a_rule_family_does_not_depend_on_the_seed() {
        let mut s = trained();
        for (a, b) in [("zero", "three"), ("one", "two"), ("two", "four"), ("three", "one"), ("one", "five")] {
            let n = |w: &str| crate::data::NUM.iter().position(|x| *x == w).unwrap();
            s.learn(&format!("what is {a} added to {b}"), crate::data::NUM[n(a) + n(b)], Some("added")).unwrap();
        }
        let lessons = |name: &str| s.units.iter().find(|u| u.name == name).unwrap().lessons.clone();
        let family: Vec<Item> = ["sum", "after", "double", "half", "minus"].iter().flat_map(|n| lessons(n)).collect();
        let added = lessons("added");
        for seed in 1..=24u64 {
            assert!(crate::discover::can_merge(&family, &added, &s.weights, seed), "seed {seed}");
        }
        // and the other way round: multiplication does not follow the family's rule, whatever the seed
        let times = lessons("times");
        for seed in 1..=24u64 {
            assert!(!crate::discover::can_merge(&family, &times, &s.weights, seed), "seed {seed}: times must never join the sum family");
        }
    }

    #[test]
    fn learning_one_lesson_at_a_time_keeps_the_tables_exactly_what_a_rebuild_would_give() {
        let mut s = trained();
        assert!(s.tables_equal_a_rebuild(), "fresh school");
        // new facts, a new phrasing, a correction, the same lesson again, a surprising lesson in a rule unit
        s.learn("capital of atlantis", "poseidonis", Some("capital")).unwrap();
        s.learn("what is the capital of atlantis", "poseidonis", Some("capital")).unwrap();
        assert!(s.tables_equal_a_rebuild(), "after new facts");
        s.learn("what is two added to three", "five", Some("sum")).unwrap();
        s.learn("what is one added to one", "two", Some("sum")).unwrap();
        assert!(s.tables_equal_a_rebuild(), "after a new phrasing");
        s.learn("capital of atlantis", "atlantica", Some("capital")).unwrap();
        s.learn("capital of atlantis", "atlantica", Some("capital")).unwrap();
        assert!(s.tables_equal_a_rebuild(), "after a correction and a repeat");
        // a new kind of lesson: no unit named, so it waits in the pen, then opens a unit of its own
        for n in 0..=6usize {
            let _ = s.learn_anywhere(&format!("what is {} less than ten", crate::data::NUM[n]), crate::data::NUM[10 - n]);
        }
        assert!(s.units.iter().any(|u| u.name.contains("less")), "a unit opened");
        assert!(s.tables_equal_a_rebuild(), "after a new unit opened");
        s.learn("what is three less than ten", "seven", None).unwrap();
        assert!(s.tables_equal_a_rebuild(), "after a lesson placed by routing");
    }

    #[test]
    fn units_taught_separately_that_follow_one_rule_become_one() {
        let mut s = trained();
        let units = s.units.len();
        for (a, b) in [("zero", "three"), ("one", "two"), ("two", "four"), ("three", "one"), ("one", "five"), ("four", "four"), ("two", "two")] {
            let n = |w: &str| crate::data::NUM.iter().position(|x| *x == w).unwrap();
            s.learn(&format!("what is {a} added to {b}"), crate::data::NUM[n(a) + n(b)], Some("added")).unwrap();
        }
        assert_eq!(s.units.len(), units + 1);
        let report = s.reorganize();
        assert!(report.merged.iter().any(|(_, gone)| gone == "added"), "'added' should have been folded into the rule family: {:?}", report.merged);
        assert!(s.units.len() <= units, "{} units after merging (had {units})", s.units.len());
        assert_eq!(s.ask("what is five added to three").answer.as_deref(), Some("eight"));
        assert_eq!(s.ask("nine minus four").answer.as_deref(), Some("five"));
        assert!(s.ask("three minus five").answer.is_none(), "merging must not make it answer a negative result");
    }

    #[test]
    fn one_wrong_lesson_told_after_school_does_not_change_the_answer_and_a_skewed_rule_does_not_override_what_was_taught() {
        let mut s = trained();
        assert_eq!(s.ask("what is double three").answer.as_deref(), Some("six"));
        // a teacher's slip: one phrasing of a known fact, taught wrongly
        let _ = s.learn("what is double three", "seven", Some("double"));
        for q in ["what is double three", "double three", "three doubled", "what is three doubled"] {
            let a = s.ask(q).answer;
            assert!(a.as_deref() == Some("six") || a.is_none(), "{q:?} -> {a:?}: a single wrong lesson must not win");
        }
        assert_eq!(s.ask("what is double four").answer.as_deref(), Some("eight"), "other facts are untouched");
        // memory and the rule disagree about a question that WAS taught: it declines rather than pick
        let u = s.units.iter().position(|x| x.name == "sum").unwrap();
        for l in s.units[u].lessons.iter_mut().filter(|l| l.query.contains("one plus two")) {
            l.answer = "five".into();
        }
        s.units[u].remember = Remembered::from_items(&s.units[u].lessons);
        let r = s.ask("what is one plus two");
        assert!(r.answer.is_none(), "rule says three, the lessons say five: {:?} ({})", r.answer, r.how);
        assert!(r.how.contains("disagree"));
    }

    fn numbers_school(f: impl Fn(i64) -> i64, upto: i64) -> School {
        let items: Vec<Item> = (0..=upto).map(|a| Item { unit: 0, fact: a as usize, template: 0, query: format!("what is zork {a}"), answer: f(a).to_string() }).collect();
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        s.set_careful(false);
        // what it would have learned from earlier kinds: a ten, and the tens and units digits as pieces
        s.weights.lib.learn_constant(10);
        s.weights.lib.learn_program(&crate::program::Expr::from_text("(div A K10)").unwrap(), 1, crate::program::NO_ORIGIN);
        s.weights.lib.learn_program(&crate::program::Expr::from_text("(mod A K10)").unwrap(), 1, crate::program::NO_ORIGIN);
        s.relearn_unit(0);
        s
    }

    #[test]
    fn it_answers_where_its_explanations_agree_declines_where_they_differ_and_asks_the_number_that_settles_it() {
        // digit sum taught on 0-15: "tens + units" and "(first + tens) mod 10" fit every lesson and part ways at 19
        let mut s = numbers_school(|a| a / 10 + a % 10, 15);
        assert_eq!(s.units[0].chosen, Mech::Program);
        assert_eq!(s.ask("what is zork 12").answer.as_deref(), Some("3"), "they agree here");
        assert_eq!(s.ask("what is zork 17").answer.as_deref(), Some("8"));
        assert!(s.ask("what is zork 19").answer.is_none(), "they part ways here: the lessons do not decide it");
        let wonders = s.wonder();
        assert!(!wonders.is_empty());
        let w = &wonders[0];
        assert!(w.unit == "zork" && w.question.starts_with("what is zork "), "{}", w.question);
        let n: i64 = w.question.rsplit(' ').next().unwrap().parse().unwrap();
        assert!(n > 15, "asks about a number nobody taught: {n}");
        // told the answer, it settles, and has nothing more to ask
        s.learn_wondered_sure(&w.question, &(n / 10 + n % 10).to_string(), "zork").unwrap();
        assert_eq!(s.ask("what is zork 29").answer.as_deref(), Some("11"));
        assert_eq!(s.ask("what is zork 19").answer.as_deref(), Some("10"));
        assert!(s.wonder().is_empty());
    }

    #[test]
    fn a_unit_that_only_memorizes_is_not_re_decided_twice_for_one_told_answer() {
        // 29 lessons nothing can explain; the 30th is told through `learn_wondered`, which used to re-decide the unit again right after `learn` had just done so.
        let answers = ["red", "blue", "green", "gold", "pink", "teal", "grey", "navy", "lime", "plum"];
        let items: Vec<Item> = (0..29usize).map(|i| Item { unit: 0, fact: i, template: 0, query: format!("what is code {}", i + 1), answer: answers[(i * 7 + i / 3) % 10].to_string() }).collect();
        let mut a = School::from_items(&items, vec!["code".into()], None, 1, true);
        let mut b = School::from_items(&items, vec!["code".into()], None, 1, true);
        assert!(a.memorize_only_known(0));
        // a: the public path; b: what it used to do, spelled out (learn, then re-decide the unit again)
        a.learn_wondered_sure("what is code 30", "plum", "code").unwrap();
        b.learn("what is code 30", "plum", Some("code")).unwrap();
        b.relearn_unit(0);
        assert_eq!((a.units[0].chosen, a.units[0].lessons.len()), (b.units[0].chosen, b.units[0].lessons.len()));
        assert_eq!(a.units[0].best_evidence(), b.units[0].best_evidence());
        for i in [1usize, 15, 29, 30] {
            let q = format!("what is code {i}");
            assert_eq!(a.ask(&q).answer, b.ask(&q).answer, "{q}");
        }
    }

    #[test]
    fn a_confirming_answer_extends_a_settled_program_without_refitting_it() {
        // the tens digit, taught on 0..14: a settled exact program. Told 24 -> 2 (it already says so) the program is not refitted or re-decided, but the lesson is
        // kept and the range it may state facts in grows to reach it.
        let mut s = numbers_school(|a| a / 10, 14);
        s.set_careful(true);
        assert_eq!(s.units[0].program().as_deref(), Some("(first div 10)"));
        assert!(s.settled_program_explaining_all(0));
        assert!(s.ask("what is zork 24").answer.is_none(), "beyond what it was taught: a guess, not a fact");
        s.learn_wondered_sure("what is zork 24", "2", "zork").unwrap();
        assert!(s.settled_program_explaining_all(0));
        assert_eq!(s.units[0].program().as_deref(), Some("(first div 10)"));
        assert_eq!(s.ask("what is zork 24").answer.as_deref(), Some("2"), "the confirmation is kept");
        assert_eq!(s.ask("what is zork 21").answer.as_deref(), Some("2"), "and the range it may speak in has grown");
        // a contradiction is NOT confirming: the program no longer explains every lesson and the full path runs
        s.units[0].lessons.push(Item { unit: 0, fact: 99, template: 0, query: "what is zork 17".into(), answer: "9".into() });
        assert!(!s.settled_program_explaining_all(0));
    }

    #[test]
    fn a_fresh_fit_that_explains_a_wrongly_told_lesson_is_a_repair_with_the_old_law_kept_as_a_rival() {
        // The tens digit (settled), then 28 -> 8 told wrongly (true: 2). `b apart ((a + b) mod 10)` also says 8 there and is the tens digit until the digits sum past
        // nine, so a fresh exact fit offers it. It used to replace the settled law outright and 19 was stated as 9 (found by the adaptive exam's wrong-teacher tier).
        // The fresh fit is now adopted only as a repair: the wrong lesson does not vouch for its neighbors, and where the old law disagrees it is a guess.
        let mut s = numbers_school(|a| a / 10, 14);
        s.set_careful(true);
        s.weights.lib.learn_constant(3);
        assert_eq!(s.units[0].program().as_deref(), Some("(first div 10)"));
        // (the lessons go straight in: this small fixture's own re-decision would fall back to plain memory)
        for (i, (q, a)) in [("what is zork 24", "2"), ("what is zork 28", "8")].into_iter().enumerate() {
            s.units[0].lessons.push(Item { unit: 0, fact: 100 + i, template: 0, query: q.to_string(), answer: a.to_string() });
        }
        // the fresh fit the re-decision would find (this fixture's own search finds the true law, so the coincidence is offered by hand)
        let coincidence = crate::program::Expr::from_text("(apart (mod A K10) (mod (add (div A K10) (mod A K10)) K10))");
        let Some(expr) = coincidence else { panic!("the expression syntax changed: update this test") };
        let fresh = Fitted::Program(Some(crate::program::ProgramRule::rebuild(expr, 1, Vec::new())));
        let (keep, state) = s.apply_rebuild_policy(0, false, fresh);
        assert!(!keep);
        match &state {
            Fitted::Program(Some(p)) => {
                assert!(p.rebuilt && p.before.as_deref() == Some("program"), "adopted as a repair");
                assert_eq!(p.repaired, vec!["what is zork 28".to_string()], "the wrong lesson is marked unproven");
            }
            _ => panic!("expected a program"),
        }
        // an old law that explains everything (nothing contradicted) is left to the ordinary decision
        let again = crate::program::Expr::from_text("(div A K10)").unwrap();
        let fresh2 = Fitted::Program(Some(crate::program::ProgramRule::rebuild(again, 1, Vec::new())));
        let (_, plain) = s.apply_rebuild_policy(0, false, fresh2);
        assert!(matches!(&plain, Fitted::Program(Some(p)) if !p.rebuilt && p.repaired.is_empty()));
    }

    #[test]
    fn a_unit_with_one_explanation_checks_it_where_it_was_tested_least_and_corrects_itself() {
        // digit sum taught on 0-15 without 8 and 9, and knowing only a ten: "first mod 9" is the one smallest explanation, and it is wrong at 9
        let items: Vec<Item> = (0..=15i64).filter(|a| *a != 8 && *a != 9).map(|a| Item { unit: 0, fact: a as usize, template: 0, query: format!("what is zork {a}"), answer: (a / 10 + a % 10).to_string() }).collect();
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        s.set_careful(false);
        s.weights.lib.learn_constant(10);
        s.relearn_unit(0);
        assert_eq!(s.units[0].chosen, Mech::Program);
        assert!(s.wonder().is_empty(), "one explanation: nothing competes, so nothing to ask about");
        assert_eq!(s.ask("what is zork 9").answer.as_deref(), Some("0"), "and it is confidently wrong at the number nobody taught");
        let checks = s.check_myself(3);
        let asked: Vec<&str> = checks.iter().map(|c| c.question.as_str()).collect();
        assert_eq!(asked, ["what is zork 8", "what is zork 9", "what is zork 30"], "the gaps inside what it was taught, then well past it");
        // told 8 it agrees; told 9 it is refuted and refits
        s.learn_wondered_sure("what is zork 8", "8", "zork").unwrap();
        assert_eq!(s.ask("what is zork 9").answer.as_deref(), Some("0"));
        s.learn_wondered_sure("what is zork 9", "9", "zork").unwrap();
        assert_eq!(s.ask("what is zork 9").answer.as_deref(), Some("9"));
        // refuted, it no longer says 0 at 9 (asserted above), whatever it now holds...
        // ...and with pieces for the tens and the units, which it would have from other kinds, it can see the rivals
        s.weights.lib.learn_program(&crate::program::Expr::from_text("(div A K10)").unwrap(), 1, crate::program::NO_ORIGIN);
        s.weights.lib.learn_program(&crate::program::Expr::from_text("(mod A K10)").unwrap(), 1, crate::program::NO_ORIGIN);
        s.reconsider_open();
        // now two explanations fit (tens + units, and (first + tens) mod 10): it declines at 29 and wonders about 19
        let w = s.wonder();
        assert!(w.iter().any(|w| w.question == "what is zork 19"), "{:?}", w.iter().map(|w| &w.question).collect::<Vec<_>>());
        s.learn_wondered_sure("what is zork 19", "10", "zork").unwrap();
        assert_eq!(s.ask("what is zork 29").answer.as_deref(), Some("11"), "two questions of its own choosing, and it has the right explanation");
    }

    #[test]
    fn a_program_that_looked_settled_is_reviewed_when_later_vocabulary_gives_it_a_rival() {
        // fitted knowing only a ten: "first mod 9" is the one smallest explanation of a digit sum taught on 0-15 (8 and 9 missing)
        let items: Vec<Item> = (0..=15i64).filter(|a| *a != 8 && *a != 9).map(|a| Item { unit: 0, fact: a as usize, template: 0, query: format!("what is zork {a}"), answer: (a / 10 + a % 10).to_string() }).collect();
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        s.set_careful(false);
        s.weights.lib.learn_constant(10);
        s.relearn_unit(0);
        assert!(matches!(&s.units[0].state, Fitted::Program(Some(p)) if p.alts.is_empty()), "one explanation, as far as it knows");
        // later kinds teach it the tens digit and the units digit; with them a rival fits just as well
        s.weights.lib.learn_program(&crate::program::Expr::from_text("(div A K10)").unwrap(), 1, crate::program::NO_ORIGIN);
        s.weights.lib.learn_program(&crate::program::Expr::from_text("(mod A K10)").unwrap(), 1, crate::program::NO_ORIGIN);
        s.review_programs();
        assert!(matches!(&s.units[0].state, Fitted::Program(Some(p)) if !p.alts.is_empty()), "now it knows it has competing explanations: {} ev {:?}", s.describe_unit_open(0), s.units[0].evidence);
        assert!(s.ask("what is zork 9").answer.is_none(), "and no longer claims 0 at 9");
    }

    #[test]
    fn a_unit_left_open_is_reconsidered_when_its_vocabulary_grows() {
        let items: Vec<Item> = (0..=25i64).filter(|a| *a != 8 && *a != 22).map(|a| Item { unit: 0, fact: a as usize, template: 0, query: format!("what is zork {a}"), answer: (a / 10 + a % 10).to_string() }).collect();
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        s.set_careful(false);
        assert_eq!(s.units[0].chosen, Mech::Remember, "without a ten it cannot build this");
        assert!(s.ask("what is zork 29").answer.is_none());
        s.weights.lib.learn_constant(10);
        s.weights.lib.learn_program(&crate::program::Expr::from_text("(div A K10)").unwrap(), 1, crate::program::NO_ORIGIN);
        s.weights.lib.learn_program(&crate::program::Expr::from_text("(mod A K10)").unwrap(), 1, crate::program::NO_ORIGIN);
        s.reconsider_open();
        assert_eq!(s.units[0].chosen, Mech::Program);
        assert_eq!(s.ask("what is zork 29").answer.as_deref(), Some("11"));
    }

    fn sums_school(f: impl Fn(i64, i64) -> i64) -> School {
        let mut items = Vec::new();
        for a in 0..=12i64 {
            for b in 0..=12i64 {
                items.push(Item { unit: 0, fact: items.len(), template: 0, query: format!("what is {a} zork {b}"), answer: f(a, b).to_string() });
            }
        }
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        s.set_careful(false);
        s
    }

    #[test]
    fn a_rule_that_changes_regime_beyond_what_was_taught_is_confidently_wrong_unless_it_is_careful_and_checks() {
        // a sum that saturates at 30: exact on everything taught (largest sum 24), wrong beyond
        let capped = |a: i64, b: i64| (a + b).min(30);
        let mut s = sums_school(capped);
        assert_eq!(s.units[0].chosen, Mech::Rule);
        assert_eq!(s.ask("what is 7 zork 9").answer.as_deref(), Some("16"));
        assert_eq!(s.ask("what is 20 zork 15").answer.as_deref(), Some("35"), "the hazard: nothing it was taught says the truth is 30");
        // careful: inside what it was taught it answers, beyond it it says so
        s.set_careful(true);
        assert_eq!(s.ask("what is 7 zork 9").answer.as_deref(), Some("16"));
        let r = s.ask("what is 20 zork 15");
        assert!(r.answer.is_none() && r.how.contains("state it as fact"), "{}", r.how);
        assert_eq!(r.guess.map(|g| g.answer).as_deref(), Some("35"), "the hazardous value is now only ever a labeled guess");
        // it asks about the edge; the first probe refutes the rule
        let checks = s.check_myself(3);
        let asked: Vec<&str> = checks.iter().map(|c| c.question.as_str()).collect();
        assert_eq!(&asked[..2], ["what is 24 zork 12", "what is 12 zork 24"], "each operand pushed to about twice its largest taught value");
        s.learn_wondered_sure("what is 24 zork 12", "30", "zork").unwrap();
        assert!(s.ask("what is 20 zork 15").answer.is_none(), "refuted: it no longer trusts a rule that said 36 where the truth is 30");
        assert_ne!(s.ask("what is 24 zork 12").answer.as_deref(), Some("36"));
    }

    #[test]
    fn one_correction_of_a_saturating_sum_teaches_the_new_regime_not_just_that_the_old_rule_is_wrong() {
        let mut s = sums_school(|a, b| (a + b).min(30));
        s.set_careful(true);
        let before = s.weights.lib.to_lines();
        s.learn("what is 24 zork 12", "30", Some("zork")).unwrap();
        assert_eq!(s.units[0].chosen, Mech::Program, "{}", s.describe_unit_open(0));
        assert!(!s.units[0].refuted, "it explains everything it was told now");
        // a labeled guess, no longer withheld, and the right one; never stated as fact beyond what was checked
        for q in ["what is 20 zork 15", "what is 28 zork 17", "what is 5 zork 29"] {
            let r = s.ask(q);
            assert!(r.answer.is_none(), "{q}: not a fact unchecked");
            assert_eq!(r.guess.map(|g| g.answer).as_deref(), Some("30"), "{q}");
        }
        assert_eq!(s.ask("what is 24 zork 12").answer.as_deref(), Some("30"), "what it was told");
        assert_eq!(s.ask("what is 7 zork 9").answer.as_deref(), Some("16"), "inside the taught range, as before");
        assert_eq!(s.weights.lib.to_lines(), before, "the temporary numbers never enter the library");
        // and a question it must still refuse
        assert!(s.ask("what is 24 zork").answer.is_none() && s.ask("what is 24 zork").guess.is_none());
    }

    #[test]
    fn one_correction_of_a_wrapping_sum_finds_the_wrap() {
        let mut s = sums_school(|a, b| (a + b) % 40);
        s.set_careful(true);
        s.learn("what is 24 zork 20", "4", Some("zork")).unwrap();
        assert_eq!(s.units[0].chosen, Mech::Program, "{}", s.describe_unit_open(0));
        assert_eq!(s.ask("what is 30 zork 25").guess.map(|g| g.answer).as_deref(), Some("15"));
        assert_eq!(s.ask("what is 24 zork 20").answer.as_deref(), Some("4"));
    }

    #[test]
    fn a_repair_built_around_one_point_never_states_a_fact_that_point_cannot_vouch_for() {
        // the truth reflects at 30 (36 -> 24, 40 -> 20); a lone correction "24 + 12 is 24" is just as well explained by a cap at 24,
        // which would say 22 + 12 is 24 (the truth is 26) if the corrected lesson vouched for the questions below it
        let mut s = sums_school(|a, b| if a + b <= 30 { a + b } else { 60 - (a + b) });
        s.set_careful(true);
        s.learn("what is 24 zork 12", "24", Some("zork")).unwrap();
        for q in ["what is 22 zork 12", "what is 20 zork 15", "what is 13 zork 20", "what is 24 zork 7"] {
            assert!(s.ask(q).answer.is_none(), "{q}: only a guess");
        }
        assert_eq!(s.ask("what is 24 zork 12").answer.as_deref(), Some("24"), "but what it was told, it knows");
        // a second, third correction near it: the law may change again, and it must not state what was never checked
        s.learn("what is 22 zork 12", "26", Some("zork")).unwrap();
        assert_eq!(s.ask("what is 22 zork 12").answer.as_deref(), Some("26"));
        assert!(s.ask("what is 21 zork 14").answer.is_none());
    }

    #[test]
    fn a_repaired_program_and_what_it_leaves_out_of_its_reach_survive_saving() {
        let mut s = sums_school(|a, b| (a + b).min(30));
        s.set_careful(true);
        s.learn("what is 24 zork 12", "30", Some("zork")).unwrap();
        let dir = crate::platform::temp_dir().join(format!("repaired_program_{}", crate::platform::unique_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.school").to_string_lossy().to_string();
        s.save(&path).unwrap();
        let back = School::load(&path).unwrap();
        assert_eq!(back.ask("what is 20 zork 15").guess.map(|g| g.answer).as_deref(), Some("30"));
        assert!(back.ask("what is 22 zork 12").answer.is_none(), "still not vouched for after loading");
        assert_eq!(back.ask("what is 24 zork 12").answer.as_deref(), Some("30"));
        assert_eq!(back.ask("what is 7 zork 9").answer.as_deref(), Some("16"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_small_unit_keeps_its_repair_when_one_lesson_fits_two_laws_and_one_question_chooses() {
        // 15 lessons, the truth is min(2a, 30): told "29 -> 30" fits `min(a + a, 30)` and `a + a mod 28` equally; one correction must not
        // send a unit this small back to plain memory (one lesson in sixteen is more than a rule tolerates), and it must ask where they differ
        let items: Vec<Item> = (0..=14i64).map(|a| Item { unit: 0, fact: a as usize, template: 0, query: format!("what is zork {a}"), answer: (2 * a).min(30).to_string() }).collect();
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        s.set_careful(true);
        s.learn_wondered_sure("what is zork 29", "30", "zork").unwrap();
        assert_eq!(s.units[0].chosen, Mech::Program, "{}", s.describe_unit_open(0));
        assert!(matches!(&s.units[0].state, Fitted::Program(Some(p)) if !p.alts.is_empty() && !p.repaired.is_empty()));
        assert!(s.ask("what is zork 20").answer.is_none(), "two explanations part ways at 20: no fact");
        let asked = s.check_myself(3);
        assert!(!asked.is_empty() && asked[0].unit == "zork", "it asks where they differ");
        let q = asked[0].question.clone();
        s.learn_wondered_sure(&q, &(2 * q.rsplit(' ').next().unwrap().parse::<i64>().unwrap()).min(30).to_string(), "zork").unwrap();
        assert_eq!(s.ask("what is zork 20").guess.map(|g| g.answer).or(s.ask("what is zork 20").answer).as_deref(), Some("30"), "one answer chose the law");
    }

    #[test]
    fn a_wrap_that_shows_itself_only_at_a_corner_is_repaired_even_though_each_operand_was_seen_separately() {
        // checks at (24, 11) and (12, 22) are still sums; (24, 22) wraps (46 -> 6). Each operand has been seen at 24 and 22, so the per-operand box
        // contains the corner, but no lesson is at least as large in both: the contradiction is outside what the others vouch for
        let mut s = sums_school(|a, b| (a + b) % 40);
        s.set_careful(true);
        s.learn("what is 24 zork 11", "35", Some("zork")).unwrap();
        s.learn("what is 12 zork 22", "34", Some("zork")).unwrap();
        s.learn("what is 24 zork 22", "6", Some("zork")).unwrap();
        assert_eq!(s.units[0].chosen, Mech::Program, "{}", s.describe_unit_open(0));
        assert_eq!(s.ask("what is 30 zork 25").guess.map(|g| g.answer).as_deref(), Some("15"));
        // the two ordinary checks vouch for what lies under them; the corner it was repaired around does not
        assert_eq!(s.ask("what is 12 zork 20").answer.as_deref(), Some("32"), "under the check at (12, 22)");
        assert!(s.ask("what is 20 zork 20").answer.is_none(), "only the corner is at least this large in both places");
        assert_eq!(s.ask("what is 24 zork 22").answer.as_deref(), Some("6"), "told");
        assert!(s.ask("what is 23 zork 22").answer.is_none(), "not vouched for by the corner");
    }

    #[test]
    fn routing_corrections_learned_from_mistakes_fix_what_counting_gets_wrong_and_are_deterministic() {
        // three units that share most of their words (a story's furniture) and differ in one cue each; the big unit's furniture swamps the small ones
        let line = NumberLine::from_counting(&crate::quantity::counting_lessons());
        let furniture = ["mary", "had", "some", "red", "apples", "and", "then", "gave", "away", "how", "many", "are", "left"];
        let mut samples: Vec<(usize, Vec<String>)> = Vec::new();
        for i in 0..90usize {
            let (u, cue) = match i % 9 {
                0..=5 => (0usize, ["more", "total", "altogether"][i % 3]),
                6 | 7 => (1, ["fewer", "left"][i % 2]),
                _ => (2, "each"),
            };
            let mut words: Vec<String> = furniture.iter().enumerate().filter(|(k, _)| (k + i) % 3 != 0).map(|(_, w)| w.to_string()).collect();
            words.push(cue.to_string());
            words.sort();
            samples.push((u, words));
        }
        let mut table = Routing { rows: HashMap::new(), lessons: Vec::new(), cells: 0, delta: HashMap::new(), pure: false };
        for (u, sh) in &samples {
            table.add_lesson(*u, sh);
        }
        let loo_hits = |t: &Routing| samples.iter().filter(|(u, sh)| School::best_and_margin(&t.scores_without(sh, *u)).0 == *u).count();
        let counting = loo_hits(&table);
        table.pure = true;
        table.refine(&samples, ROUTE_EPOCHS);
        let learned = samples.iter().filter(|(u, sh)| School::best_and_margin(&table.scores(sh, &mut Ledger::default())).0 == *u).count();
        assert!(learned > counting, "corrections must beat counting here: {learned} vs {counting} of {}", samples.len());
        assert!(learned * 10 >= samples.len() * 9, "and route nearly all of them: {learned}");
        // deterministic: the same lessons give the same corrections
        let mut again = Routing { rows: HashMap::new(), lessons: table.lessons.clone(), cells: 0, delta: HashMap::new(), pure: true };
        again.refine(&samples, ROUTE_EPOCHS);
        let mut a: Vec<_> = table.delta.iter().collect();
        let mut b: Vec<_> = again.delta.iter().collect();
        a.sort_by(|x, y| x.0.cmp(y.0));
        b.sort_by(|x, y| x.0.cmp(y.0));
        assert_eq!(a, b);
        let _ = &line;
    }

    #[test]
    fn a_selector_and_the_scales_of_its_stated_rates_survive_saving_and_cut_the_question_down_when_asked() {
        let mut s = sums_school(|a, b| a + b);
        let l = s.number_line().clone();
        let ex: Vec<(String, Vec<usize>)> = (0..40usize).map(|i| (format!("what is {} zork {} and also {} zorf", 1 + i % 9, 2 + i % 7, 3 + i % 5), vec![0, 1])).collect();
        s.set_selector(crate::selector::Selector::train(&ex, &l), ex.clone());
        s.set_guess_scales([0.8, 0.4]);
        let before = s.focus("what is 7 zork 5 and also 9 zorf");
        assert!(before.contains(crate::selector::BLANK), "{before}");
        let dir = crate::platform::temp_dir().join(format!("selector_school_{}", crate::platform::unique_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.school").to_string_lossy().to_string();
        s.save(&path).unwrap();
        let back = School::load(&path).unwrap();
        assert_eq!(back.focus("what is 7 zork 5 and also 9 zorf"), before);
        assert_eq!(back.guess_scales(), [0.8, 0.4]);
        // a question with two numbers is read as written
        assert_eq!(back.focus("what is 7 zork 5"), "what is 7 zork 5");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_program_right_on_most_inputs_but_not_reliable_enough_to_state_facts_may_only_guess() {
        // sums with a third of the answers wrong (a deterministic pattern): the plurality of each input's lessons is still the sum, but nothing here is reliable
        let mut items = Vec::new();
        for a in 0..=9i64 {
            for b in 0..=9i64 {
                for copy in 0..3usize {
                    let truth = a + b;
                    let wrong = (a * 7 + b * 3 + copy as i64) % 3 == 0 && copy == 0;
                    items.push(Item { unit: 0, fact: items.len(), template: copy, query: format!("what is {a} zork {b} please{copy}"), answer: if wrong { (truth + 3).to_string() } else { truth.to_string() } });
                }
            }
        }
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        s.set_careful(false);
        let r = s.ask("what is 4 zork 5 please0");
        let Fitted::Program(Some(p)) = &s.units[0].state else { panic!("expected a program: {}", s.describe_unit_open(0)) };
        assert!(s.units[0].best_evidence() < mechanisms::reliable(), "not reliable: {}", s.units[0].best_evidence());
        assert!(p.partial.is_some(), "below the bar it is flagged as guess-only");
        assert!(r.answer.is_none(), "a provisional program never states a fact: {:?}", r.answer);
        assert_eq!(r.guess.as_ref().map(|g| g.answer.as_str()), Some("9"), "and it guesses the plurality's answer");
    }

    #[test]
    fn an_answer_it_is_told_is_filed_where_a_program_reproduces_it_and_one_it_cannot_place_is_not_taken_in() {
        let mut s = trained();
        let times = s.units.iter().position(|u| u.name == "times").unwrap();
        let before: usize = s.units.iter().map(|u| u.lessons.len()).sum();
        // three numbers, only two matter: 2 x 3 = 6 and no other pair makes six by any unit's program
        let (unit, pair) = s.place_told("what is two times three and also five", "six").expect("one unit, one pair");
        assert_eq!((unit, pair), (times, vec![0, 1]));
        // an answer no program makes from any of its numbers is not placed
        assert!(s.place_told("what is two times three and also five", "four").is_none() || s.units[s.place_told("what is two times three and also five", "four").unwrap().0].name != "times");
        let r = s.learn_told("what is two times three and also five", "seven");
        assert!(r.is_err(), "nothing makes seven out of 2, 3 or 5 by a program it knows: it is not taken in");
        assert_eq!(s.units.iter().map(|u| u.lessons.len()).sum::<usize>(), before, "and no lesson was added");
        // placed: the question is filed cut down to the numbers that were used
        s.learn_told("what is two times three and also five", "six").ok();
        let filed: Vec<&str> = s.units[times].lessons.iter().map(|l| l.query.as_str()).filter(|q| q.contains(crate::selector::BLANK)).collect();
        assert!(filed.iter().all(|q| q.contains("two") && q.contains("three")), "{filed:?}");
        assert_eq!(s.units.iter().map(|u| u.lessons.len()).sum::<usize>() >= before, true);
    }

    #[test]
    fn a_repaired_program_is_not_swapped_for_a_coincidence_by_the_next_lesson_and_the_checks_do_not_vouch_for_a_wrong_law() {
        // the building blocks a unit like this would have from other kinds: a three, and a sum of two squares, which make 18 = 3x3 + 3x3
        let mut s = sums_school(|a, b| (a + b).min(30));
        s.set_careful(true);
        s.weights.lib.learn_constant(3);
        s.weights.lib.learn_program(&crate::program::Expr::from_text("(add (mul A A) (mul B B))").unwrap(), 2, crate::program::NO_ORIGIN);
        s.learn("what is 24 zork 12", "30", Some("zork")).unwrap();
        // a second probe that a coincidence law (second + (first min 18)) also gets right: 18 + 12 = 30
        s.learn("what is 48 zork 12", "30", Some("zork")).unwrap();
        let Fitted::Program(Some(p)) = &s.units[0].state else { panic!("expected a program: {}", s.describe_unit_open(0)) };
        assert!(!p.repaired.is_empty(), "it must still be the repaired program: {}", p.expr.show());
        // and nothing below the probes is stated as a fact it cannot back: the truth under (48, 12) at (19, 2) is 21
        let r = s.ask("what is 19 zork 2");
        assert!(r.answer.as_deref().map_or(true, |a| a == "21"), "stated {:?}, truth 21 ({})", r.answer, p.expr.show());
    }

    #[test]
    fn a_rebuilt_law_does_not_overrule_the_one_it_replaced_until_lessons_told_after_the_repair_have_separated_them() {
        let mut s = sums_school(|a, b| (a + b).min(30));
        s.set_careful(true);
        s.learn("what is 24 zork 12", "30", Some("zork")).unwrap(); // the repair: min(first + second, 30)
        s.learn("what is 48 zork 12", "30", Some("zork")).unwrap(); // the first lesson since that the old law gets wrong and the new one right
        // (30, 10) lies under (48, 12), which vouches for it, but the old law says 40 and the new one 30: one lesson is not yet enough
        let r = s.ask("what is 30 zork 10");
        assert!(r.answer.is_none(), "not yet a fact: {:?}", r.answer);
        assert_eq!(r.guess.map(|g| g.answer).as_deref(), Some("30"), "but the rebuilt law may guess");
        assert_eq!(s.ask("what is 48 zork 12").answer.as_deref(), Some("30"), "what it was told is a fact whatever the old law says");
        // a second lesson that separates them retires the old law
        s.learn("what is 12 zork 48", "30", Some("zork")).unwrap();
        assert_eq!(s.ask("what is 30 zork 10").answer.as_deref(), Some("30"), "now the rebuilt law speaks as fact where it is vouched for");
    }

    #[test]
    fn a_contradiction_that_no_small_program_explains_changes_nothing_it_did_not_before() {
        // arbitrary corrections ("24 + 12 is 7", "24 + 13 is 30"): a lone 7 would be a wrap at 29 and is found, but nothing small fits both
        let mut s = sums_school(|a, b| a + b);
        s.set_careful(true);
        s.learn("what is 24 zork 12", "7", Some("zork")).unwrap();
        s.learn("what is 24 zork 13", "30", Some("zork")).unwrap();
        assert!(s.units[0].refuted, "still refuted: no repair was possible: {}", s.describe_unit_open(0));
        assert!(s.ask("what is 20 zork 15").guess.is_none(), "and so still no guess out there");
    }

    #[test]
    fn beyond_what_it_was_taught_it_offers_a_labeled_guess_never_a_fact_and_still_refuses_what_it_must() {
        let s = School::train(&kinds::world(1), 1, true); // careful, as by default
        // inside what it was taught: a fact, no guess
        let r = s.ask("what is two times three");
        assert_eq!(r.answer.as_deref(), Some("six"));
        assert!(r.guess.is_none());
        // beyond it: not a fact, a labeled guess, and the guess says how far out it is
        let r = s.ask("what is seven times one");
        assert!(r.answer.is_none(), "a guess must never be counted as an answer");
        let g = r.guess.expect("a rule it trusts, applied past the taught numbers, is offered as a guess");
        assert_eq!(g.answer, "seven");
        assert!(g.stretch > 0.0);
        assert!(r.how.contains("won't state it as fact"), "{}", r.how);
        // in digits, the guess follows the question's style
        let r = s.ask("what is 17 plus 25");
        assert!(r.answer.is_none());
        assert_eq!(r.guess.map(|g| g.answer).as_deref(), Some("42"));
        // the must-refuse battery gets no guess either: no unreadable numbers, no negative results, no unmet words, nothing it only memorizes
        for q in [
            "three minus five",
            "two minus nine",
            "what is -3 plus 5",
            "what is 3.5 plus 2",
            "what is 50% of ten",
            "what is blorp plus three",
            "what is the capital of atlantis",
            "what is the code for five",
            "which is bigger banana or seven",
        ] {
            let r = s.ask(q);
            assert!(r.answer.is_none() && r.guess.is_none(), "{q:?} should be refused plainly, got answer {:?} guess {:?}", r.answer, r.guess);
        }
        // permissive states it as a fact instead, and offers no separate guess
        let mut p = School::train(&kinds::world(1), 1, true);
        p.set_careful(false);
        let r = p.ask("what is seven times one");
        assert_eq!(r.answer.as_deref(), Some("seven"));
        assert!(r.guess.is_none());
    }

    #[test]
    fn a_correction_of_a_guess_ends_the_guessing_out_there_and_a_confirmation_extends_the_reach() {
        // the truth saturates at 30; nothing taught (sums up to 24) says so
        let mut s = sums_school(|a, b| (a + b).min(30));
        s.set_careful(true);
        let r = s.ask("what is 20 zork 15");
        assert_eq!(r.guess.map(|g| g.answer).as_deref(), Some("35"), "a labeled guess, wrong");
        // told the truth about a neighbor (an ordinary `learn`, not a check it asked for)
        s.learn("what is 24 zork 12", "30", Some("zork")).unwrap();
        // (it now builds the law that fits everything it was told, min(first + second, 30): the guess there is now the right one, still not a fact;
        // when no small program fits the contradictions it may not even guess there: `a_contradiction_that_no_small_program_explains...`)
        let again = s.ask("what is 20 zork 15");
        assert!(again.answer.is_none(), "{}", again.how);
        assert_eq!(again.guess.map(|g| g.answer).as_deref(), Some("30"));
        assert_eq!(s.ask("what is 7 zork 9").answer.as_deref(), Some("16"), "inside what the rule explains it still speaks as fact");
        assert!(s.ask("what is 24 zork 12").answer.as_deref() == Some("30"), "what it was told it knows");
        let stated: Vec<String> = ["what is 22 zork 12", "what is 20 zork 12", "what is 13 zork 13"].iter().filter_map(|q| s.ask(q).answer).collect();
        assert!(stated.is_empty(), "the correction must not widen the range the rule may state as fact: {stated:?}");
        let snap = s.reach_snapshot();
        s.settle_all_reach();
        assert_eq!(snap, s.reach_snapshot(), "no stale record of what the rule explains");

        // a rule that holds: a confirmed guess is kept and extends where it may state facts
        let mut s = sums_school(|a, b| a + b);
        s.set_careful(true);
        assert!(s.ask("what is 20 zork 15").answer.is_none());
        s.learn("what is 20 zork 15", "35", Some("zork")).unwrap();
        assert_eq!(s.ask("what is 20 zork 15").answer.as_deref(), Some("35"), "told, so a fact");
        assert_eq!(s.ask("what is 18 zork 14").answer.as_deref(), Some("32"), "inside the extended range it is a fact");
        assert!(s.ask("what is 40 zork 15").answer.is_none(), "still not past what was confirmed");
        assert_eq!(s.ask("what is 40 zork 15").guess.map(|g| g.answer).as_deref(), Some("55"));
        let snap = s.reach_snapshot();
        s.settle_all_reach();
        assert_eq!(snap, s.reach_snapshot());
    }

    #[test]
    fn confirming_two_edges_does_not_vouch_for_the_corner_between_them() {
        // the truth saturates at 30; sums up to 24 were taught
        let truth = |a: i64, b: i64| (a + b).min(30);
        let mut s = sums_school(truth);
        s.set_careful(true);
        // two confirmed guesses, each stretching one operand: (20, 2) and (2, 20) are right (22) whether or not the sum saturates
        s.learn("what is 20 zork 2", "22", Some("zork")).unwrap();
        s.learn("what is 2 zork 20", "22", Some("zork")).unwrap();
        // inside the box those two stretch, but nothing near (18, 18) was ever checked: the rule says 36, the truth is 30
        let r = s.ask("what is 18 zork 18");
        assert!(r.answer.is_none(), "stated as fact, and false: {:?}", r.answer);
        assert_eq!(r.guess.map(|g| g.answer).as_deref(), Some("36"), "a labeled guess instead");
        // what is covered stays covered
        assert_eq!(s.ask("what is 20 zork 1").answer.as_deref(), Some("21"));
        assert_eq!(s.ask("what is 7 zork 9").answer.as_deref(), Some("16"));
        assert_eq!(s.ask("what is 12 zork 12").answer.as_deref(), Some("24"));
    }

    /// `n` units that all saturate at 30 (taught sums up to 24), told apart only by their operator word: a world where guessing out there fails.
    fn capped_units(n: usize) -> School {
        let mut items = Vec::new();
        for u in 0..n {
            for a in 0..=12i64 {
                for b in 0..=12i64 {
                    items.push(Item { unit: u, fact: items.len(), template: 0, query: format!("what is {a} zork{u} {b}"), answer: (a + b).min(30).to_string() });
                }
            }
        }
        let names: Vec<String> = (0..n).map(|u| format!("zork{u}")).collect();
        let mut s = School::from_items(&items, names, None, 1, true);
        s.set_careful(true);
        s
    }

    #[test]
    fn it_keeps_a_record_of_its_own_guesses_states_it_and_stops_guessing_where_it_has_been_wrong_more_often_than_right() {
        let mut s = capped_units(8);
        // an untried guesser states a modest rate and says how little it has to go on
        let g = s.ask("what is 20 zork0 15").guess.expect("a guess");
        assert_eq!(g.answer, "35");
        assert_eq!(g.tried, 0);
        assert!((g.rate - 0.5).abs() < 1e-6, "{}", g.rate);
        // told the truth about four such guesses, each wrong, in four different units
        for u in 0..4 {
            s.learn(&format!("what is 20 zork{u} 15"), "30", Some(&format!("zork{u}"))).unwrap();
        }
        let (hit, miss) = s.guess_record()[stretch_bucket(g.stretch)];
        assert_eq!((hit, miss), (0, 4));
        // a fifth unit's guess at that distance is now withheld, and says why
        let r = s.ask("what is 20 zork5 15");
        assert!(r.answer.is_none() && r.guess.is_none(), "{:?}", r.guess);
        assert!(r.how.contains("wrong more often than right"), "{}", r.how);
        // a different distance has its own record: a much nearer guess is still offered
        let near = s.ask("what is 14 zork6 3").guess;
        assert!(near.is_some(), "{}", s.ask("what is 14 zork6 3").how);
        // the record survives saving and loading
        let dir = crate::platform::temp_dir().join(format!("guess_record_{}", crate::platform::unique_id()));
        let path = dir.join("a.school").to_string_lossy().to_string();
        s.save(&path).unwrap();
        assert_eq!(School::load(&path).unwrap().guess_record(), s.guess_record());
        std::fs::remove_dir_all(dir).ok();

        // in a world where the rule holds, confirmations raise the stated rate
        let mut t = sums_school(|a, b| a + b);
        t.set_careful(true);
        for (a, b) in [(14, 3), (15, 3), (16, 3), (13, 4), (14, 4), (15, 4)] {
            t.learn(&format!("what is {a} zork {b}"), &(a + b).to_string(), Some("zork")).unwrap();
        }
        let g = t.ask("what is 17 zork 5").guess.expect("still a guess a little further out");
        assert!(g.tried >= 5 && g.rate > 0.8, "tried {} rate {}", g.tried, g.rate);
    }

    #[test]
    fn a_relation_that_fits_most_lessons_is_a_partial_program_whose_answers_are_only_ever_guesses() {
        // 100 lessons: 88 follow "how far apart", 12 follow something else; no exact program exists
        let mut items = Vec::new();
        for a in 0..10i64 {
            for b in 0..10i64 {
                let exception = (a * 10 + b) % 8 == 3;
                let ans = if exception { (a + b) * 2 + 1 } else { (a - b).abs() };
                items.push(Item { unit: 0, fact: items.len(), template: 0, query: format!("what is {a} zork {b}"), answer: ans.to_string() });
            }
        }
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        assert_eq!(s.units[0].chosen, Mech::Program, "a relation that fits most of what it was taught is worth having as a guess");
        let r = s.ask("what is 3 zork 8");
        assert!(r.answer.is_none(), "never a fact: {:?}", r.answer);
        let g = r.guess.expect("a guess");
        assert_eq!(g.answer, "5");
        assert!(g.rate > 0.6 && g.rate < 1.0, "it states how sure it is: {}", g.rate);
        assert!(r.how.contains("only a guess"), "{}", r.how);
        // a word it never met does not stop a partial program from guessing (it was never required to know every word)
        assert_eq!(s.ask("what is 3 zork 8 blorp").guess.map(|g| g.answer).as_deref(), Some("5"));
        // it does not guess with the wrong number of numbers
        assert!(s.ask("what is 3 zork 8 and 4").guess.is_none());
        // a partial program is not vocabulary for later units, and is not checked at the edges like an exact one
        assert!(s.library_text().trim().is_empty() || !s.library_text().contains("apart"), "{}", s.library_text());
        assert!(s.check_myself(3).is_empty());
        // it survives saving and loading
        let dir = crate::platform::temp_dir().join(format!("partial_program_{}", crate::platform::unique_id()));
        let path = dir.join("a.school").to_string_lossy().to_string();
        s.save(&path).unwrap();
        let back = School::load(&path).unwrap();
        let g2 = back.ask("what is 3 zork 8").guess.expect("still a guess after loading");
        assert_eq!(g2.answer, "5");
        assert!(back.ask("what is 3 zork 8").answer.is_none());
        std::fs::remove_dir_all(dir).ok();
        s.set_careful(false);
        assert!(s.ask("what is 3 zork 8").answer.is_none(), "permissive does not turn a guess into a fact either");
    }

    #[test]
    fn a_table_with_no_relation_is_not_explained_by_a_constant_that_fits_the_common_answer() {
        // primes below 60: "no" fits four lessons in five, and says nothing
        let is_prime = |n: u32| n >= 2 && (2..n).all(|d| n % d != 0);
        let items: Vec<Item> = (0..60u32)
            .map(|n| Item { unit: 0, fact: n as usize, template: 0, query: format!("what is zork {n}"), answer: if is_prime(n) { "one".into() } else { "zero".into() } })
            .collect();
        let s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        assert_ne!(s.units[0].chosen, Mech::Program, "a constant is not an explanation of primality");
        assert!(s.ask("what is zork 61").guess.is_none() && s.ask("what is zork 61").answer.is_none());
    }

    /// A unit of sums taught on 0..=5 in number WORDS: the number line ends at ten, so a sum past ten cannot be said.
    fn word_sums_school() -> School {
        let w = crate::data::NUM;
        let mut items = Vec::new();
        for a in 0..=5usize {
            for b in 0..=5usize {
                items.push(Item { unit: 0, fact: items.len(), template: 0, query: format!("what is {} zork {}", w[a], w[b]), answer: w[a + b].to_string() });
            }
        }
        School::from_items(&items, vec!["zork".into()], None, 1, true)
    }

    #[test]
    fn when_the_far_probe_cannot_be_said_it_probes_the_farthest_that_can() {
        let s = word_sums_school();
        assert_eq!(s.ask("what is zero zork six").answer, None, "beyond what was taught: not a fact yet");
        let checks = s.check_myself(4);
        assert!(!checks.is_empty(), "a number line that ends at ten must not leave it with nothing to check");
        let w = crate::data::NUM;
        for c in &checks {
            let nums: Vec<usize> = crate::data::tokenize(&c.question).iter().filter_map(|t| w.iter().position(|n| n == t)).collect();
            assert!(nums.iter().sum::<usize>() <= 10, "{:?}: a probe whose answer has no number word cannot be answered", c.question);
        }
        assert!(checks.iter().any(|c| crate::data::tokenize(&c.question).iter().any(|t| ["six", "seven", "eight", "nine", "ten"].contains(&t.as_str()))), "{:?}", checks.iter().map(|c| &c.question).collect::<Vec<_>>());
    }

    #[test]
    fn answering_its_own_checks_clears_the_refusals_it_can_clear_and_never_states_a_wrong_fact() {
        let mut s = word_sums_school();
        let w = crate::data::NUM;
        let truth = |q: &str| -> Option<usize> {
            let n: Vec<usize> = crate::data::tokenize(q).iter().filter_map(|t| w.iter().position(|x| x == t)).collect();
            (n.len() == 2 && n[0] + n[1] <= 10).then(|| n[0] + n[1])
        };
        let probes = |s: &School| -> Vec<String> { (0..=10usize).flat_map(|a| (0..=10usize).map(move |b| format!("what is {} zork {}", w[a], w[b]))).filter(|q| truth(q).is_some()).collect::<Vec<_>>().into_iter().filter(|_| true).map(|q| { let _ = s; q }).collect() };
        let before = probes(&s).iter().filter(|q| s.ask(q).answer.is_some()).count();
        for _ in 0..6 {
            for c in s.check_myself(4) {
                if let Some(a) = truth(&c.question) {
                    s.learn_wondered_sure(&c.question, w[a], "zork").unwrap();
                }
            }
        }
        let mut wrong = 0;
        let all = probes(&s);
        let after = all.iter().filter(|q| s.ask(q).answer.is_some()).count();
        for q in &all {
            if let Some(a) = s.ask(q).answer {
                if Some(a.as_str()) != truth(q).map(|t| w[t]) {
                    wrong += 1;
                }
            }
        }
        assert_eq!(wrong, 0);
        assert!(after > before + 20, "answered as fact: {before} before the checks, {after} after, of {}", all.len());
        assert_eq!(after, all.len(), "every question the number line can express is covered after a few rounds");
    }

    #[test]
    fn teaching_more_never_shrinks_what_a_unit_may_state_as_fact() {
        // a template word ("equals") is glue while it is in a large share of the facts and stops being glue as other facts are added; the
        // reach used to depend on that, so the check answers (phrased without it) stopped counting and coverage FELL as it learned
        let mut items = Vec::new();
        for a in 0..=5i64 {
            for b in 0..=5i64 {
                items.push(Item { unit: 0, fact: items.len(), template: 0, query: format!("what is {a} zork {b}"), answer: (a + b).to_string() });
                items.push(Item { unit: 0, fact: items.len(), template: 2, query: format!("{a} zork {b} equals"), answer: (a + b).to_string() });
            }
        }
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        assert!(s.ask("7 zork 4 equals").answer.is_none(), "beyond what was taught");
        // teach many facts phrased without the template word, as the checks do, covering up to 12 + 12
        for a in 6..=12i64 {
            for b in 0..=12i64 {
                s.learn(&format!("what is {a} zork {b}"), &(a + b).to_string(), Some("zork")).unwrap();
            }
        }
        s.learn("what is 12 zork 12", "24", Some("zork")).unwrap();
        assert_eq!(s.ask("7 zork 4 equals").answer.as_deref(), Some("11"), "covered by what was learned, whatever the phrasing");
        assert_eq!(s.ask("what is 7 zork 4").answer.as_deref(), Some("11"));
        assert_eq!(s.ask("3 zork 2 equals").answer.as_deref(), Some("5"));
    }

    #[test]
    fn in_a_merged_family_one_operation_is_not_vouched_for_by_the_lessons_of_another() {
        // plus taught on 0..=5, minus only on 0..=3: asking about minus beyond 3 must not be covered by the plus lessons
        let mut items = Vec::new();
        for a in 0..=5i64 {
            for b in 0..=5i64 {
                items.push(Item { unit: 0, fact: items.len(), template: 0, query: format!("what is {a} plus {b}"), answer: (a + b).to_string() });
            }
        }
        for a in 0..=3i64 {
            for b in 0..=a {
                items.push(Item { unit: 0, fact: items.len(), template: 1, query: format!("what is {a} minus {b}"), answer: (a - b).to_string() });
            }
        }
        let s = School::from_items(&items, vec!["arith".into()], None, 1, true);
        assert_eq!(s.units[0].chosen, Mech::Rule, "one rule with an operator per word");
        assert_eq!(s.ask("what is 4 plus 5").answer.as_deref(), Some("9"), "plus was taught that far");
        let r = s.ask("what is 5 minus 1");
        assert!(r.answer.is_none(), "minus was only taught to 3: {:?}", r.answer);
        assert_eq!(r.guess.map(|g| g.answer).as_deref(), Some("4"), "a labeled guess instead");
        assert_eq!(s.ask("what is 3 minus 2").answer.as_deref(), Some("1"));
    }

    #[test]
    fn the_recognition_floor_follows_the_margins_as_it_learns_and_never_rises_above_where_it_was_taught() {
        let mut s = capped_units(8);
        assert!(s.route_ratio > 0.0 && s.route_base.is_finite(), "calibrated when taught");
        let base = s.route_floor;
        assert_eq!(base, s.route_base);
        let lessons_before = s.calibrated_lessons;
        // teach one unit a few hundred more lessons, as the baby's own checks do
        // (lessons that fit the rule: 327 sums up to 30 that were not taught; ones that break it would force a slow full re-fit each time)
        for a in 0..=30i64 {
            for b in 0..=(30 - a) {
                if a > 12 || b > 12 {
                    s.learn(&format!("what is {a} zork0 {b}"), &(a + b).to_string(), Some("zork0")).unwrap();
                }
            }
        }
        assert!(s.calibrated_lessons > lessons_before, "it recalibrated by itself after enough new lessons");
        assert!(s.route_floor <= base + 1e-6, "never above the floor it was taught with: {} vs {base}", s.route_floor);
        // what it must still refuse, it refuses: nothing like any unit it knows
        for q in ["what is blorp quux", "tell me a story about dragons", "what is zork0 zork5 zork6"] {
            let r = s.ask(q);
            assert!(r.answer.is_none() && r.guess.is_none(), "{q:?} must stay refused: {:?} {:?}", r.answer, r.guess);
        }
        // and what it knows is recognized
        assert_eq!(s.ask("what is 7 zork3 4").answer.as_deref(), Some("11"));
        // the calibration survives saving and loading
        let dir = crate::platform::temp_dir().join(format!("route_calibration_{}", crate::platform::unique_id()));
        let path = dir.join("a.school").to_string_lossy().to_string();
        s.save(&path).unwrap();
        let back = School::load(&path).unwrap();
        assert!((back.route_ratio - s.route_ratio).abs() < 1e-6 && (back.route_base - s.route_base).abs() < 1e-6);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn leaving_a_lesson_out_can_only_make_it_harder_to_recognize() {
        let s = capped_units(8);
        let m = s.loo_margins();
        assert!(!m.is_empty());
        // the same lessons routed normally (each is in the table, so it is recognized at least as well)
        let mut plain: Vec<f32> = s.units.iter().flat_map(|u| u.lessons.iter().map(|l| s.route(&l.query, &mut Ledger::default()).1)).collect();
        plain.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = |v: &[f32]| v[v.len() / 2];
        assert!(median(&m) <= median(&plain) + 1e-4, "leave-one-out median {} vs plain {}", median(&m), median(&plain));
    }

    #[test]
    fn a_skeptical_baby_holds_an_answer_that_contradicts_a_settled_rule_until_it_is_repeated() {
        let mut s = sums_school(|a, b| a + b);
        s.set_careful(true);
        s.set_skeptical(true);
        // a slip: the teacher says 19 for 8 + 9 once, then the right answer (17) when asked again
        let l = s.learn_wondered("what is 8 zork 9", "19", "zork").unwrap();
        assert!(l.disputed, "a contradiction of a settled rule is held, not learned");
        assert_eq!(s.disputed_count(), 1);
        assert!(s.ask("what is 8 zork 9").answer.as_deref() != Some("19"), "nothing was learned from the held answer");
        let l = s.learn_wondered("what is 8 zork 9", "17", "zork").unwrap();
        assert!(!l.disputed, "the second answer agrees with the rule: the first was a slip");
        assert_eq!(s.disputed_count(), 0);
        // a real change: the same surprising answer twice is believed
        let l = s.learn_wondered("what is 11 zork 9", "30", "zork").unwrap();
        assert!(l.disputed);
        let l = s.learn_wondered("what is 11 zork 9", "30", "zork").unwrap();
        assert!(!l.disputed, "said twice: believed");
        assert_eq!(s.disputed_count(), 0);
        // a third version is still unsettled
        let l = s.learn_wondered("what is 12 zork 9", "40", "zork").unwrap();
        assert!(l.disputed);
        let l = s.learn_wondered("what is 12 zork 9", "41", "zork").unwrap();
        assert!(l.disputed, "a different answer the second time: still unsettled");
        // (on by default; switched off, the answer is simply learned)
        let mut t = sums_school(|a, b| a + b);
        t.set_careful(true);
        assert!(t.learn_wondered("what is 8 zork 9", "19", "zork").unwrap().disputed, "skeptical by default");
        t.set_skeptical(false);
        assert!(!t.learn_wondered("what is 8 zork 9", "19", "zork").unwrap().disputed, "and trusting when switched off");
    }

    #[test]
    fn an_answer_held_for_a_second_opinion_survives_saving_so_two_separate_runs_can_confirm_it() {
        let mut s = sums_school(|a, b| a + b);
        s.set_careful(true);
        assert!(s.learn_wondered("what is 8 zork 9", "19", "zork").unwrap().disputed);
        let dir = crate::platform::temp_dir().join(format!("disputed_{}", crate::platform::unique_id()));
        let path = dir.join("a.school").to_string_lossy().to_string();
        s.save(&path).unwrap();
        let mut back = School::load(&path).unwrap();
        assert_eq!(back.disputed_count(), 1, "the held answer is in the file");
        assert!(!back.learn_wondered("what is 8 zork 9", "19", "zork").unwrap().disputed, "said again in a later run: believed");
        assert_eq!(back.disputed_count(), 0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_settled_program_survives_a_few_contradicting_lessons_but_not_a_change_of_regime() {
        // "how far apart" taught on 0..=12: an exact program
        let apart = |a: i64, b: i64| (a - b).abs();
        let mut items = Vec::new();
        for a in 0..=12i64 {
            for b in 0..=12i64 {
                items.push(Item { unit: 0, fact: items.len(), template: 0, query: format!("what is {a} zork {b}"), answer: apart(a, b).to_string() });
            }
        }
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        s.set_careful(true);
        let exact = |s: &School| matches!(&s.units[0].state, Fitted::Program(Some(p)) if p.partial.is_none());
        assert!(exact(&s) && s.units[0].chosen == Mech::Program);
        // one wrong lesson inside the taught range: an outlier, not a new law
        s.learn("what is 7 zork 3", "9", Some("zork")).unwrap();
        assert!(exact(&s), "one lesson against 169 that agree must not turn an exact program into a guessing one");
        assert_eq!(s.ask("what is 10 zork 4").answer.as_deref(), Some("6"), "facts elsewhere are untouched");
        // a contradiction outside the taught range: still no guess beyond it
        s.learn("what is 20 zork 3", "99", Some("zork")).unwrap();
        assert!(exact(&s));
        let r = s.ask("what is 25 zork 3");
        assert!(r.answer.is_none() && r.guess.is_none(), "a lesson out there has shown the program wrong: {:?} {:?}", r.answer, r.guess);
        assert_eq!(s.ask("what is 12 zork 5").answer.as_deref(), Some("7"), "inside what it explains it still speaks as fact");
        // many contradictions are a change of regime, not outliers: it re-decides
        for k in 0..8i64 {
            s.learn(&format!("what is {} zork 1", 14 + k), "0", Some("zork")).unwrap();
        }
        assert!(!exact(&s) || s.units[0].chosen != Mech::Program, "eight contradictions are not outliers");
    }

    #[test]
    fn careful_is_the_default_and_survives_saving_and_loading() {
        let items: Vec<Item> = (0..=12i64)
            .flat_map(|a| (0..=12i64).map(move |b| (a, b)))
            .enumerate()
            .map(|(i, (a, b))| Item { unit: 0, fact: i, template: 0, query: format!("what is {a} zork {b}"), answer: (a + b).to_string() })
            .collect();
        let mut s = School::from_items(&items, vec!["zork".into()], None, 1, true);
        assert!(s.ask("what is 20 zork 15").answer.is_none(), "beyond what it was taught: not stated as fact by default");
        assert_eq!(s.ask("what is 7 zork 9").answer.as_deref(), Some("16"));
        let dir = crate::platform::temp_dir().join(format!("careful_default_{}", crate::platform::unique_id()));
        let path = dir.join("a.school").to_string_lossy().to_string();
        s.save(&path).unwrap();
        assert!(School::load(&path).unwrap().ask("what is 20 zork 15").answer.is_none(), "a saved baby stays careful");
        // permissive is remembered too, and a file with no careful line at all (older files) loads careful
        s.set_careful(false);
        s.save(&path).unwrap();
        assert_eq!(School::load(&path).unwrap().ask("what is 20 zork 15").answer.as_deref(), Some("35"));
        let text = std::fs::read_to_string(&path).unwrap().replace("careful 0\n", "");
        std::fs::write(&path, text).unwrap();
        assert!(School::load(&path).unwrap().ask("what is 20 zork 15").answer.is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_rule_that_holds_has_its_reach_extended_by_the_checks_it_asks_for() {
        let mut s = sums_school(|a, b| a + b);
        s.set_careful(true);
        assert!(s.ask("what is 20 zork 15").answer.is_none(), "unchecked");
        // the two edges alone (each operand pushed out, the other at its largest) do not vouch for the corner between them
        let checks = s.check_myself(3);
        for q in checks.iter().take(2) {
            let n: Vec<i64> = q.question.split_whitespace().filter_map(|w| w.parse().ok()).collect();
            s.learn_wondered_sure(&q.question, &(n[0] + n[1]).to_string(), "zork").unwrap();
        }
        assert!(s.ask("what is 20 zork 15").answer.is_none(), "two edges are not the corner");
        assert_eq!(s.ask("what is 20 zork 15").guess.map(|g| g.answer).as_deref(), Some("35"));
        // the third check is the corner (both large at once)
        let corner = &checks[2];
        assert!(corner.why.contains("both"), "{}", corner.why);
        let n: Vec<i64> = corner.question.split_whitespace().filter_map(|w| w.parse().ok()).collect();
        s.learn_wondered_sure(&corner.question, &(n[0] + n[1]).to_string(), "zork").unwrap();
        assert_eq!(s.units[0].chosen, Mech::Rule, "every check was confirmed");
        assert_eq!(s.ask("what is 20 zork 15").answer.as_deref(), Some("35"), "now it may answer out there");
        assert!(s.ask("what is 100 zork 15").answer.is_none(), "but not much further than it has checked");
    }

    #[test]
    fn it_asks_nothing_about_a_table_that_has_no_explanation_and_never_pretends_to_know() {
        let table = [7, 3, 9, 0, 5, 10, 2, 8, 1, 6, 4, 12, 11, 15, 14, 13];
        let s = numbers_school(|a| table[a as usize], 15);
        assert_eq!(s.units[0].chosen, Mech::Remember);
        assert!(s.wonder().is_empty(), "no competing explanations, so nothing to ask");
        assert!(s.ask("what is zork 20").answer.is_none());
    }

    #[test]
    fn what_it_learned_to_build_with_survives_saving_and_loading() {
        let mut s = trained();
        s.weights.lib.learn_constant(10);
        s.weights.lib.learn_constant(7);
        let tens = crate::program::Expr::from_text("(div A K10)").unwrap();
        assert!(s.weights.lib.learn_program(&tens, 1, 0));
        let sumsq = crate::program::Expr::from_text("(add (mul A A) (mul B B))").unwrap();
        assert!(s.weights.lib.learn_program(&sumsq, 2, 1));
        let path = "runs/test-library.school";
        s.save(path).unwrap();
        let t = School::load(path).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(t.weights.lib, s.weights.lib);
        assert_eq!(t.weights.lib.consts, vec![10, 7]);
        assert_eq!(t.weights.lib.funcs.len(), 2);
        // a rebuild of the tables (a merge, say) must not lose it either
        let mut u = t;
        u.rebuild_tables();
        assert_eq!(u.weights.lib, s.weights.lib);
    }

    #[test]
    fn saving_and_loading_changes_nothing() {
        let s = trained();
        let path = crate::platform::temp_dir().join(format!("baby_school_{}.school", crate::platform::unique_id()));
        let path = path.to_str().unwrap().to_string();
        s.save(&path).unwrap();
        let back = School::load(&path).unwrap();
        for u in 0..s.units.len() {
            std::fs::remove_file(format!("{path}.unit{u}.model")).ok();
        }
        std::fs::remove_file(&path).ok();
        for q in [
            "what is seven times one", "nine minus four", "which is smaller ten or four", "how far apart are two and ten",
            "capital of peru", "what comes after eight", "what is double three", "what is the code for four", "capital of atlantis",
            "what color is the sky", "opposite of hot", "what does the cat say", "what is five plus four",
        ] {
            let (a, b) = (s.ask(q), back.ask(q));
            assert_eq!(a.answer, b.answer, "{q}");
            assert_eq!(a.how, b.how, "{q}");
        }
    }

    #[test]
    fn the_precomputed_table_routes_as_well_as_comparing_with_every_lesson() {
        let w = kinds::world(1);
        let s = School::train(&w, 1, true);
        let mut wrong = 0;
        for it in w.reworded.iter().chain(&w.new_items).chain(&w.beyond) {
            if s.route(&it.query, &mut Ledger::default()).0 != it.unit {
                wrong += 1;
            }
        }
        assert_eq!(wrong, 0, "misrouted {wrong}");
    }

    #[test]
    fn after_school_it_learns_a_new_phrasing_and_a_new_kind() {
        let mut s = trained();
        // Before: "multiplied by" is not a phrase it has met.
        assert!(s.ask("what is four multiplied by two").answer.is_none());
        // Told which unit it belongs to, a phrasing joins it. The unit still learns by program.
        for (q, a) in [("what is two multiplied by three", "six"), ("one multiplied by five", "five"), ("what is five multiplied by two", "ten")] {
            s.learn(q, a, Some("times")).unwrap();
        }
        let times = s.units.iter().find(|u| u.name == "times").unwrap();
        assert_eq!(times.chosen, Mech::Program, "still builds the rule");
        assert_eq!(s.ask("what is four multiplied by two").answer.as_deref(), Some("eight"));
        assert_eq!(s.ask("what is two times four").answer.as_deref(), Some("eight"), "what it knew before still works");

        // A new kind: it opens a unit, remembers until it has enough facts, then works out how to learn it.
        let first = s.learn("what is zero less than ten", "ten", Some("lessthan")).unwrap();
        assert!(first.opened && first.now == Mech::Remember);
        for (n, a) in [("one", "nine"), ("two", "eight"), ("three", "seven"), ("four", "six"), ("five", "five")] {
            s.learn(&format!("what is {n} less than ten"), a, Some("lessthan")).unwrap();
        }
        let last = s.learn("six less than ten", "four", Some("lessthan")).unwrap();
        assert_ne!(last.now, Mech::Remember, "with seven facts it should have found something that generalizes: {:?}", last.now);
        assert_eq!(s.ask("what is eight less than ten").answer.as_deref(), Some("two"));
    }
}




