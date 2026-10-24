// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 DawsMacs
//! What the baby has been *told*, kept as plain text and consulted before anything else.
//!
//! The network can only read the words it was trained on, and only say the answers it was trained to say. This memory has no such
//! limit: it stores the spelled-out question and answer, so it can learn a brand-new word and a brand-new answer instantly, with no
//! weight update, and it keeps them across runs in a file you can read and edit. It is the "write it down" rung of the learning
//! ladder, opened up to the whole vocabulary.

use crate::data::tokenize;
use crate::ledger::Ledger;
use crate::store::Store;
use std::collections::HashMap;
use std::io::Write;

pub struct Memory {
    path: String,
    /// Every distinct word seen in a taught question; id = position + 1 (0 is reserved for padding in `Store`).
    words: Vec<String>,
    index: HashMap<String, usize>,
    answers: Vec<String>,
    answer_index: HashMap<String, usize>,
    store: Store,
    facts: usize,
}

impl Memory {
    pub fn open(path: &str, threshold: f32) -> Memory {
        let mut m = Memory {
            path: path.to_string(),
            words: Vec::new(),
            index: HashMap::new(),
            answers: Vec::new(),
            answer_index: HashMap::new(),
            store: Store::new(threshold),
            facts: 0,
        };
        if let Ok(text) = std::fs::read_to_string(path) {
            for line in text.lines() {
                if let Some((answer, query)) = line.split_once('\t') {
                    m.insert(query, answer);
                }
            }
        }
        m
    }

    pub fn len(&self) -> usize {
        self.facts
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// Has any taught question used this word?
    pub fn knows(&self, word: &str) -> bool {
        self.index.contains_key(word)
    }

    fn word_id(&mut self, w: &str) -> usize {
        if let Some(&i) = self.index.get(w) {
            return i;
        }
        self.words.push(w.to_string());
        self.index.insert(w.to_string(), self.words.len());
        self.words.len()
    }

    fn insert(&mut self, query: &str, answer: &str) {
        let ids: Vec<usize> = tokenize(query).iter().map(|w| self.word_id(w)).collect();
        let a = match self.answer_index.get(answer) {
            Some(&a) => a,
            None => {
                self.answers.push(answer.to_string());
                self.answer_index.insert(answer.to_string(), self.answers.len() - 1);
                self.answers.len() - 1
            }
        };
        let before = self.store.len();
        self.store.insert(&ids, &ids, a);
        if self.store.len() > before {
            self.facts += 1;
        }
    }

    /// Learn one fact now, and keep it. The only cost is appending a line to a file.
    pub fn teach(&mut self, query: &str, answer: &str) -> std::io::Result<()> {
        self.insert(query, answer);
        if let Some(dir) = std::path::Path::new(&self.path).parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(f, "{}\t{}", answer, tokenize(query).join(" "))
    }

    /// The taught answer whose question is closest to this one, with how close. `weight` says how much each word should count.
    pub fn lookup(&self, query: &str, weight: &dyn Fn(&str) -> f32, ledger: &mut Ledger) -> Option<(String, f32)> {
        let words = tokenize(query);
        let mut temps: Vec<String> = Vec::new();
        let ids: Vec<usize> = words
            .iter()
            .map(|w| match self.index.get(w) {
                Some(&i) => i,
                None => match temps.iter().position(|t| t == w) {
                    Some(p) => self.words.len() + 1 + p,
                    None => {
                        temps.push(w.clone());
                        self.words.len() + temps.len()
                    }
                },
            })
            .collect();
        let name = |id: usize| -> &str {
            if id <= self.words.len() { &self.words[id - 1] } else { &temps[id - self.words.len() - 1] }
        };
        let w = |id: usize| weight(name(id));
        self.store.lookup(&ids, &w, ledger).map(|(a, sim)| (self.answers[a].clone(), sim))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> String {
        crate::platform::temp_dir().join(format!("baby_mem_{}_{name}", crate::platform::unique_id())).to_str().unwrap().to_string()
    }

    fn rare(w: &str) -> f32 {
        if ["what", "is", "the", "of", "capital"].contains(&w) { 0.1 } else { 5.0 }
    }

    #[test]
    fn a_new_word_and_a_new_answer_are_learned_at_once_and_survive_a_restart() {
        let path = tmp("a");
        std::fs::remove_file(&path).ok();
        let mut m = Memory::open(&path, 0.7);
        let mut l = Ledger::default();
        assert!(m.lookup("capital of atlantis", &rare, &mut l).is_none());
        m.teach("capital of atlantis", "poseidonis").unwrap();
        assert_eq!(m.lookup("what is the capital of atlantis", &rare, &mut l).map(|x| x.0), Some("poseidonis".into()));
        assert!(m.lookup("capital of mars", &rare, &mut l).is_none(), "a different place must not retrieve it");

        let again = Memory::open(&path, 0.7);
        assert_eq!(again.len(), 1);
        assert_eq!(again.lookup("capital of atlantis", &rare, &mut l).map(|x| x.0), Some("poseidonis".into()));
        assert!(again.knows("atlantis") && !again.knows("mars"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn teaching_the_same_question_again_corrects_it() {
        let path = tmp("b");
        std::fs::remove_file(&path).ok();
        let mut m = Memory::open(&path, 0.7);
        m.teach("capital of narnia", "cairparavel").unwrap();
        m.teach("capital of narnia", "cair paravel").unwrap();
        let mut l = Ledger::default();
        assert_eq!(m.lookup("capital of narnia", &rare, &mut l).map(|x| x.0), Some("cair paravel".into()));
        assert_eq!(Memory::open(&path, 0.7).lookup("capital of narnia", &rare, &mut l).map(|x| x.0), Some("cair paravel".into()));
        std::fs::remove_file(&path).ok();
    }
}
