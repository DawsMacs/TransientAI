# Transient repo instructions

**Naming (2026-10-21):** the project and the repo are **Transient** (`DawsMacs/Transient`). **TransientAI** is the name of the *creator* of Transient (as OpenAI is to ChatGPT); Daws is its owner, founder and creator, so
copyright and license headers naming `DawsMacs` are correct. Never call the project "TransientAI"; the meaning of the name "Transient" is Daws's to tell, so never guess it in docs.

Transient is Daws's long-horizon project: a light, adaptive "system of intelligence" meant to run on ordinary consumer hardware, winning differently from cloud models (not out-scaling them). It is pursued
whether or not it works. The code is `baby/` (pure Rust, zero dependencies, builds offline).

## Start of session

1. Read `TransientHandover.md` in full (rewritten 2026-10-21: state, decisions that are Daws's, options, guards, working lessons). It has the state, the numbers, the code map, the plan and what is next, the honest caveats, and the working habits. Then `TransientVision.md` (Daws's own entries, in his
   voice: append dated entries, never rewrite them; assistant context goes in separate labeled sections).
2. Run `git pull --ff-only` and `gh pr list --state all` (pull requests wait for the user to merge them), then the two-minute check in the handover (`cargo test --release --offline`; `./target/release/baby school`; the scorecard's WRONG column must read 0.0%; `bash golden/check.sh`).

## Rules

- **Communication protocol: en-US.** All writing (code, identifiers, comments, runtime strings, docs, site copy, commit messages, replies) is en-US: spelling and vocabulary (color, behavior, license, organize, recognize, center, percent, toward). This holds from now on for all work, not only for Novice (which, rebuilt from scratch, is en-US from its first line). Stick with en-US; check new text before committing.
- **Goldens and pull requests.** The exam and adaptive-stream answers are pinned in `golden/` (`golden/check.sh`); a change that is meant to change answers runs `golden/check.sh update` in the same commit and says why. Changes to `.github/workflows` go through a pull request, and the user merges.
- **Honesty over polish.** Report WRONG separately from refused. Record mistakes and rejected hypotheses in `baby/README.md` as they happen. State caveats plainly (synthetic tidy lessons; estimated joules; mechanisms and thresholds are
  the assistant's, not discovered).
- **Pessimism is input, never a brake.** Daws wants risks phrased as design constraints/terrain/checkpoints that redirect the work. Do not propose kill criteria or stop the project over difficulty; do not claim certainty of success.
- **Absolute freedom, within the stated goals** (frontier-class intelligence on typical hardware, light on RAM/memory, built around Adaptive Learning, Rust-first, measured in joules not just parameters). Default to doing the plan.
- **After every new capability, ask adversarial questions** (the apart/minus/"N less than ten" ambiguity bit three times) and keep batteries of must-refuse questions in the tests.
- **Do not run system-setting commands** (e.g. the RAPL `chmod`): give Daws the command instead. **This is a git repo** (`origin` = `git@github.com:DawsMacs/Transient.git`, public, AGPL-3.0-only; use his global git account). He has authorized commits and pushes to `origin main` for this work; still ask before force-pushing, adding other remotes or changing repo settings. The repo is public: never commit `.local/` or details of the sibling project.
- The sibling project `~/Hiraketa` (Nana) has its own `CLAUDE.md`; its offline-mode work is Nana's private research and is separate from Transient.
