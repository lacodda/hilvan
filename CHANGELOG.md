# Changelog

All notable changes to this project are documented in this file.

## [0.8.0] - 2026-10-20

### Breaking Changes
- Every answer carries `answer_id` and `answered_at`, and is refused without them: it is taken once, at the moment its device gave it. `POST /api/formulas/{id}/check` is gone - the app checks each typed turn with the core built to WebAssembly.

### Dependencies
- Update the toolchain and dependencies

### Documentation
- Give every command a reference page, and say the status in one paragraph
- Explain the road, short sittings and this device, and bump to 0.8.0

### Features
- Take each answer once, at the moment its device gave it
- Drill out of the tutor's reach, in a few minutes, with one hand and from a keyboard

## [0.7.0] - 2026-10-15

### Breaking Changes
- The learner model file is format 2, described by learner-model-2.json, and carries `breaks`; files of format 1 are refused. The demo learner is made again with a long weekend away and an absence counted as a break.

### Documentation
- Explain breaks, leftovers and what slipped, and bump to 0.7.0

### Features
- Stop the clock for a break and let leftovers take new places
- Keep breaks in the learner model file, format 2
- Take breaks, see what slipped and how firmly a card holds

## [0.6.0] - 2026-10-12

### Dependencies
- Update the dependencies and move to pnpm 10.34.5

### Documentation
- Explain the inventory, the learner model file and the demo, and bump to 0.6.0

### Features
- Declare the words a learner knows in an inventory
- Keep the learner model in one file, and replay any past day
- Go through the commonest words, and set any day beside today

### Testing
- Hold a first no and a past card's replay to account

## [0.5.0] - 2026-10-08

### Documentation
- Explain parents, typed answers and own words, and bump to 0.5.0

### Features
- Stand formulas on parents, check typed answers and take own words
- Name each formula's parents and accept contractions
- Type a formula, see what slipped, and add words of your own

### Testing
- Drill the shipped pack by typing its first sentence

## [0.4.0] - 2026-10-01

### Bug Fixes
- Advise sleeping more, ask without a dangling slot, vary must

### Dependencies
- Update, and take Button and the type scale from dowel

### Documentation
- Explain words, the lexicon and the levels
- Say the tutor teaches words, and bump to 0.4.0

### Features
- Rank English words by how often they are met
- Learn words inside the sentences they are met in
- Ask a word in its sentence and show the levels on Today

## [0.3.1] - 2026-09-23

### Bug Fixes
- Gather each image's digests by a pattern the other image cannot match

## [0.3.0] - 2026-09-23

### Bug Fixes
- Give pip a timeout so a stalled download fails instead of hanging
- Speak a language nobody chose a voice for with the voice Piper puts first

### Dependencies
- Update the dependencies and take the accent from dowel

### Documentation
- Make the readme a shopfront
- Centre the badges and drop the duplicate heading
- Say the tutor speaks, and bump to 0.3.0

### Features
- Answer a substitution with a sentence, not the scaffold
- Speak the material with Piper and ElevenLabs behind one cache
- Run a Piper voice beside the tutor and publish its image
- Hear every sentence, listen without reading, and choose the voices

## [0.2.2] - 2026-09-03

### Bug Fixes
- Drill the form you switch to, and name the tabs honestly

## [0.2.1] - 2026-09-03

### Bug Fixes
- Wear the product mark and set the screens at a readable size

## [0.2.0] - 2026-09-03

### Documentation
- Describe forms, directions and pace, and bump to 0.2.0

### Features
- Schedule each formula in both directions and group its forms
- Put the three forms on one card and drill both directions

## [0.1.2] - 2026-09-03

### Features
- Add an install compose that pulls the published image

## [0.1.1] - 2026-09-03

### Bug Fixes
- Keep the password as a hash and sessions in the database

## [0.1.0] - 2026-09-03

### Dependencies
- Update the toolchain and dependencies

### Documentation
- Document the API, packs and a learner's first week

### Features
- Scaffold the hilvan server, SPA, docs and CI
- Add the brand assets and the product colour
- Add the learner model, formula packs and FSRS scheduling
- Serve today's queue, the drill and a door to put them behind
- Add the tutor app - today, the drill and signing in

