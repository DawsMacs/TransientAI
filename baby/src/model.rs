// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 DawsMacs
//! A trained (or training) baby: the network plus the two word lists it needs to talk. Saved as a small plain-text file you can read.

use crate::data::{tokenize, Example};
use crate::ledger::Ledger;
use crate::net::{Config, Net, Reader, Scratch};
use crate::store::Store;
use std::collections::{BTreeSet, HashMap};

pub const PAD: usize = 0;
pub const UNK: usize = 1;

pub struct Model {
    pub net: Net,
    /// Words it can read; index 0 is `<pad>`, 1 is `<unk>`.
    pub words: Vec<String>,
    /// Words it can say.
    pub answers: Vec<String>,
    /// The seed that made its dataset split, so `eval` can rebuild the same one.
    pub data_seed: u64,
    /// How many of the queries it was taught contain each word, and how many queries there were: lets it tell common words
    /// ("what", "the") from telling ones ("peru"). Zero `heard` means unknown, and every word then counts the same.
    pub df: Vec<u32>,
    pub heard: u32,
    /// The distinct questions it was taught (so it can tell "I have seen a question like this" from "I haven't").
    pub taught: Vec<String>,
    /// Calibrated on its own experience: refuse when confidence or familiarity falls below these. Zero means not calibrated.
    pub conf_floor: f32,
    pub fam_floor: f32,
    /// If the baby decided a rule serves its numeric questions better than the network, the rule's operator weights.
    pub rule: Vec<(String, [f32; 3])>,
    word_index: HashMap<String, usize>,
    answer_index: HashMap<String, usize>,
}

fn index_of(list: &[String]) -> HashMap<String, usize> {
    list.iter().enumerate().map(|(i, w)| (w.clone(), i)).collect()
}

/// Every word that appears in the training queries. Anything else the baby meets later is `<unk>`.
pub fn build_words(train: &[Example]) -> Vec<String> {
    let set: BTreeSet<String> = train.iter().flat_map(|e| tokenize(&e.query)).collect();
    let mut words = vec!["<pad>".to_string(), "<unk>".to_string()];
    words.extend(set);
    words
}

/// The value that `keep` of the numbers are at or above.
pub fn percentile_floor(mut v: Vec<f32>, keep: f32) -> f32 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[(((1.0 - keep) * v.len() as f32) as usize).min(v.len() - 1)]
}

fn hash_piece(salt: u8, piece: &str, buckets: usize) -> usize {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ salt as u64;
    h = h.wrapping_mul(0x100_0000_01b3);
    for b in piece.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    ((h >> 11) % buckets as u64) as usize
}

/// What the letters reader sees: hashed pieces of the question's own spelling. A whole word, its 3- and 4-letter groups (with
/// word-boundary marks), and each pair of neighboring words. Any word at all has pieces, and a typo keeps most of them.
pub fn letter_features(query: &str, buckets: usize) -> Vec<usize> {
    let words = tokenize(query);
    let mut out = Vec::new();
    let whole_words = std::env::var("BABY_NO_WHOLE_WORD").is_err();
    for w in &words {
        if whole_words {
            out.push(hash_piece(b'w', w, buckets));
        }
        let padded: Vec<char> = format!("<{w}>").chars().collect();
        for n in [3usize, 4] {
            if padded.len() >= n {
                for win in padded.windows(n) {
                    out.push(hash_piece(b'0' + n as u8, &win.iter().collect::<String>(), buckets));
                }
            }
        }
    }
    for pair in words.windows(2) {
        out.push(hash_piece(b'b', &format!("{} {}", pair[0], pair[1]), buckets));
    }
    out
}

pub fn build_answers(sets: &[&[Example]]) -> Vec<String> {
    let set: BTreeSet<String> = sets.iter().flat_map(|s| s.iter().map(|e| e.answer.clone())).collect();
    set.into_iter().collect()
}

impl Model {
    pub fn new(net: Net, words: Vec<String>, answers: Vec<String>, data_seed: u64) -> Model {
        let df = vec![0; words.len()];
        Model { word_index: index_of(&words), answer_index: index_of(&answers), df, heard: 0, taught: Vec::new(), conf_floor: 0.0, fam_floor: 0.0, rule: Vec::new(), net, words, answers, data_seed }
    }

    /// Record which words the training queries contained (for `word_weight`) and which questions it was taught.
    pub fn learn_familiarity(&mut self, train: &[Example]) {
        self.df = vec![0; self.words.len()];
        self.heard = 0;
        let distinct: BTreeSet<&str> = train.iter().map(|e| e.query.as_str()).collect();
        self.taught = distinct.into_iter().map(String::from).collect();
        for e in train {
            let mut ids: Vec<usize> = tokenize(&e.query).iter().filter_map(|w| self.word_index.get(w).copied()).collect();
            ids.sort_unstable();
            ids.dedup();
            for i in ids {
                self.df[i] += 1;
            }
            self.heard += 1;
        }
    }

    /// A store of the questions it was taught, for measuring how familiar a new question is.
    pub fn familiarity_store(&self) -> Store {
        let mut s = Store::new(0.0);
        for q in &self.taught {
            let w = self.word_ids(q);
            s.insert(&w, &w, 0);
        }
        s
    }

    /// 0 (nothing like it was ever taught) to 1 (the same question, give or take common words).
    pub fn familiarity(&self, store: &Store, query: &str) -> f32 {
        let ids = self.word_ids(query);
        let w = |id: usize| self.word_weight(&self.words[id]);
        store.lookup(&ids, &w, &mut Ledger::default()).map_or(0.0, |x| x.1)
    }

    /// Set the refusal thresholds from what reworded real questions look like: keep `keep` of them answerable.
    pub fn calibrate(&mut self, positives: &[Example], keep: f32, s: &mut Scratch) {
        if positives.is_empty() {
            return;
        }
        let store = self.familiarity_store();
        let mut conf = Vec::new();
        let mut fam = Vec::new();
        for e in positives {
            let p = self.net.probs(&self.encode(&e.query), s);
            conf.push(p.iter().cloned().fold(0.0, f32::max));
            fam.push(self.familiarity(&store, &e.query));
        }
        self.conf_floor = percentile_floor(conf, keep);
        self.fam_floor = percentile_floor(fam, keep);
    }

    /// Has it met this word? (Words it only ever *said*, as answers, are not words it can *read*.)
    pub fn knows(&self, word: &str) -> bool {
        self.word_index.contains_key(word) && word != "<unk>" && word != "<pad>"
    }

    /// How much a word should count when comparing questions: rare words a lot, common words little, unmet words the most.
    pub fn word_weight(&self, word: &str) -> f32 {
        if self.heard == 0 {
            return 1.0;
        }
        let df = self.word_index.get(word).map_or(0, |&i| self.df[i]);
        (((1 + self.heard) as f32) / ((1 + df) as f32)).ln().max(0.0) + 0.05
    }

    /// What the network reads for this question: word ids for the word readers, hashed spelling pieces for the letters reader.
    pub fn encode(&self, query: &str) -> Vec<usize> {
        if self.net.cfg.reader == Reader::Letters {
            return letter_features(query, self.net.cfg.buckets);
        }
        self.word_ids(query)
    }

    /// What the fact store compares: word ids for the word readers, hashed spelling pieces (shifted by one so that 0 stays
    /// free for padding) for the letters reader, which makes the store tolerant of misspellings and of words it has never met.
    pub fn key_ids(&self, query: &str) -> Vec<usize> {
        if self.net.cfg.reader == Reader::Letters {
            return letter_features(query, self.net.cfg.buckets).into_iter().map(|b| b + 1).collect();
        }
        self.word_ids(query)
    }

    /// Word ids padded to the query length.
    pub fn word_ids(&self, query: &str) -> Vec<usize> {
        let len = self.net.cfg.len;
        let mut ids: Vec<usize> =
            tokenize(query).iter().take(len).map(|w| *self.word_index.get(w).unwrap_or(&UNK)).collect();
        ids.resize(len, PAD);
        ids
    }

    pub fn answer_id(&self, answer: &str) -> Option<usize> {
        self.answer_index.get(answer).copied()
    }

    /// The `k` most likely answer words with their probabilities.
    pub fn ask(&self, query: &str, k: usize, s: &mut Scratch) -> Vec<(String, f32)> {
        let p = self.net.probs(&self.encode(query), s);
        let mut order: Vec<usize> = (0..p.len()).collect();
        order.sort_by(|&a, &b| p[b].partial_cmp(&p[a]).unwrap());
        order.into_iter().take(k).map(|i| (self.answers[i].clone(), p[i])).collect()
    }

    pub fn save(&self, path: &str) -> std::io::Result<()> {
        use std::fmt::Write;
        let c = &self.net.cfg;
        let mut out = String::new();
        writeln!(out, "BABY1").unwrap();
        writeln!(out, "data_seed {}", self.data_seed).unwrap();
        writeln!(
            out,
            "dim {} hidden {} len {} vocab {} classes {} reader {} buckets {}",
            c.dim, c.hidden, c.len, c.vocab, c.classes, c.reader.name(), c.buckets
        )
        .unwrap();
        writeln!(out, "words {}", self.words.join(" ")).unwrap();
        writeln!(out, "answers {}", self.answers.join(" ")).unwrap();
        if self.heard > 0 {
            let df: Vec<String> = self.df.iter().map(|d| d.to_string()).collect();
            writeln!(out, "df {}", df.join(" ")).unwrap();
            writeln!(out, "heard {}", self.heard).unwrap();
        }
        if !self.taught.is_empty() {
            writeln!(out, "taught {}", self.taught.len()).unwrap();
            for q in &self.taught {
                writeln!(out, "{q}").unwrap();
            }
        }
        if !self.rule.is_empty() {
            writeln!(out, "rule {}", self.rule.len()).unwrap();
            for (w, o) in &self.rule {
                writeln!(out, "{w} {} {} {}", o[0], o[1], o[2]).unwrap();
            }
        }
        if self.conf_floor > 0.0 || self.fam_floor > 0.0 {
            writeln!(out, "floors {} {}", self.conf_floor, self.fam_floor).unwrap();
        }
        writeln!(out, "params {}", self.net.theta.len()).unwrap();
        for chunk in self.net.theta.chunks(16) {
            let line: Vec<String> = chunk.iter().map(|v| format!("{v}")).collect();
            writeln!(out, "{}", line.join(" ")).unwrap();
        }
        if let Some(dir) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, out)
    }

    pub fn load(path: &str) -> Result<Model, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
        let mut lines = text.lines();
        let mut next = |what: &str| lines.next().ok_or_else(|| format!("{path}: missing {what}"));
        if next("header")? != "BABY1" {
            return Err(format!("{path}: not a BABY1 model file"));
        }
        let data_seed: u64 = next("seed")?.strip_prefix("data_seed ").ok_or("bad seed line")?.parse().map_err(|e| format!("{e}"))?;
        let dims: Vec<&str> = next("dims")?.split_whitespace().collect();
        let get = |name: &str| -> Result<usize, String> {
            let at = dims.iter().position(|d| *d == name).ok_or(format!("dims: no {name}"))?;
            dims.get(at + 1).ok_or("dims: truncated")?.parse::<usize>().map_err(|e| format!("{e}"))
        };
        // Older files have no reader: they are the original position-tied one.
        let reader = dims
            .iter()
            .position(|d| *d == "reader")
            .and_then(|at| dims.get(at + 1))
            .map(|n| Reader::parse(n).ok_or(format!("{path}: unknown reader {n}")))
            .transpose()?
            .unwrap_or(Reader::Concat);
        let buckets = get("buckets").unwrap_or(0);
        let cfg = Config { dim: get("dim")?, hidden: get("hidden")?, len: get("len")?, vocab: get("vocab")?, classes: get("classes")?, reader, buckets };
        let words: Vec<String> = next("words")?.strip_prefix("words ").ok_or("bad words line")?.split_whitespace().map(String::from).collect();
        let answers: Vec<String> = next("answers")?.strip_prefix("answers ").ok_or("bad answers line")?.split_whitespace().map(String::from).collect();
        let mut line = next("params")?;
        let (mut df, mut heard, mut taught, mut floors) = (None, 0u32, Vec::new(), (0.0f32, 0.0f32));
        let mut rule: Vec<(String, [f32; 3])> = Vec::new();
        loop {
            if let Some(rest) = line.strip_prefix("df ") {
                df = Some(rest.split_whitespace().map(|x| x.parse::<u32>()).collect::<Result<Vec<_>, _>>().map_err(|e| format!("bad df: {e}"))?);
            } else if let Some(rest) = line.strip_prefix("heard ") {
                heard = rest.parse().map_err(|e| format!("bad heard: {e}"))?;
            } else if let Some(rest) = line.strip_prefix("taught ") {
                let count: usize = rest.parse().map_err(|e| format!("bad taught count: {e}"))?;
                for _ in 0..count {
                    taught.push(next("taught question")?.to_string());
                }
            } else if let Some(rest) = line.strip_prefix("rule ") {
                let count: usize = rest.parse().map_err(|e| format!("bad rule count: {e}"))?;
                for _ in 0..count {
                    let parts: Vec<&str> = next("rule weights")?.split_whitespace().collect();
                    if parts.len() != 4 {
                        return Err(format!("{path}: bad rule line"));
                    }
                    let n = |i: usize| parts[i].parse::<f32>().map_err(|e| format!("bad rule number: {e}"));
                    rule.push((parts[0].to_string(), [n(1)?, n(2)?, n(3)?]));
                }
            } else if let Some(rest) = line.strip_prefix("floors ") {
                let v: Vec<f32> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if v.len() == 2 {
                    floors = (v[0], v[1]);
                }
            } else {
                break;
            }
            line = next("params")?;
        }
        let n: usize = line.strip_prefix("params ").ok_or("bad params line")?.parse().map_err(|e| format!("{e}"))?;
        let mut theta = Vec::with_capacity(n);
        for line in lines {
            for tok in line.split_whitespace() {
                theta.push(tok.parse::<f32>().map_err(|e| format!("bad number {tok}: {e}"))?);
            }
        }
        if theta.len() != n || n != cfg.n_params() || words.len() != cfg.vocab || answers.len() != cfg.classes {
            return Err(format!("{path}: sizes do not match the header"));
        }
        let mut model = Model::new(Net::from_theta(cfg, theta), words, answers, data_seed);
        if let Some(df) = df {
            if df.len() == model.words.len() {
                model.df = df;
                model.heard = heard;
            }
        }
        model.taught = taught;
        model.rule = rule;
        model.conf_floor = floors.0;
        model.fam_floor = floors.1;
        Ok(model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;
    use crate::rng::Rng;

    fn small_model() -> Model {
        let ds = data::build(1);
        let words = build_words(&ds.train);
        let answers = build_answers(&[&ds.train, &ds.paraphrase_test, &ds.item_test]);
        let cfg = Config { vocab: words.len(), dim: 4, len: data::LEN, hidden: 8, classes: answers.len(), reader: Reader::Bigram, buckets: 16 };
        Model::new(Net::new(cfg, &mut Rng::new(2)), words, answers, 1)
    }

    #[test]
    fn unknown_words_become_unk_and_short_queries_are_padded() {
        let m = small_model();
        let ids = m.encode("capital of atlantis");
        assert_eq!(ids.len(), data::LEN);
        assert_eq!(ids[2], UNK);
        assert!(ids[3..].iter().all(|&i| i == PAD));
    }

    #[test]
    fn save_then_load_gives_identical_answers() {
        let m = small_model();
        let path = crate::platform::temp_dir().join(format!("baby_test_{}.model", crate::platform::unique_id()));
        let path = path.to_str().unwrap();
        m.save(path).unwrap();
        let back = Model::load(path).unwrap();
        std::fs::remove_file(path).ok();
        assert_eq!(back.words, m.words);
        assert_eq!(back.answers, m.answers);
        assert_eq!(back.net.theta, m.net.theta);
        let mut s = Scratch::new(&m.net.cfg);
        assert_eq!(m.ask("what is the capital of france", 3, &mut s), back.ask("what is the capital of france", 3, &mut s));
    }
}
