# 0002 · One learner model for every language

Date: 2026-09-03. Status: accepted.

## Context

hilvan starts with English and gains Spanish later, and the material for a language depends on the learner's native language as much as on the target: the prompts of "English from Russian" are Russian sentences, and "Spanish from English" is a different pack entirely.

There are two ways to hold that. Either each language pair gets its own tables, endpoints and screens - which is how a course product usually grows - or the language is a value in one model, and a pair is data.

## Decision

**A language is a column, never a branch.** One `formula` table, one `card` table, one review history, one schedule. Rows carry the target language; the pack they came from carries the pair.

- Packs are files: `packs/en-from-ru/pack.toml` today, `packs/es-from-ru/pack.toml` later. The native language of the learner is a property of a pack, not of the code.
- Nothing in the server matches on a language code to decide behaviour. Where a language genuinely differs - tokenisation, a voice, a text-to-speech `language_code` - the difference is looked up by code, not written as an `if`.
- Screens and the API take a language as a parameter, and default to the one language when there is only one.

## Consequences

- **Adding a language is adding a file.** v1.1.0 "Spanish" is a pack and a voice, not a second half of the product. The work already done - repetition, drills, audio, the reader - applies to it on the day it lands.
- **The learner's progress is comparable across languages.** One card table means "what is due today" is one query, and a session can mix languages if the learner ever wants that.
- **Russian is data, never a string in the code.** The prompts, explanations and glosses of the Russian-speaking learner live in the pack; the interface stays English. This is what keeps a public repository publishable and the product translatable.
- **The cost is a little indirection now**: a pack loader, ids that must be unique across packs, and a language table that holds one row of interest for months.
