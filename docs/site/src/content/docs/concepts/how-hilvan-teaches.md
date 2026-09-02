---
title: How hilvan teaches
description: Three mechanisms behind the tutor - comprehensible input, spaced retrieval, and production with feedback - and what the LLM's role actually is.
---

hilvan is built on three mechanisms that show up again and again in language acquisition research, not on a theory of its own.

## Comprehensible input, slightly above level

Material you can read or hear that sits just past what you already know teaches you the most per minute: known words and structures carry the meaning, and the few unknown pieces are learnable from context. Material that is too easy teaches nothing; material that is too hard teaches nothing either, because there is no foothold. hilvan compiles sentences and lessons that target this narrow band on purpose, rather than handing you material written for nobody in particular.

## Spaced retrieval

A formula or a word drilled once is forgotten on a predictable schedule; recalling it right before it would have faded resets that schedule and pushes the next review further out. hilvan uses FSRS to decide what to review and when, so review time goes to what is about to be forgotten rather than what was already reviewed yesterday.

## Production with feedback

Recognising a form is easier than producing it, and producing it is what speaking and writing actually require. Grammar formulas are drilled by substitution - the same pattern, different words - so production practice happens on structures you can already half-manage, with feedback close enough in time to correct the model you are building rather than reinforce a mistake.

## The LLM compiles material; it does not chat

An LLM assembles sentences, drills and lessons from your learner model - the formulas and words you already know - so that new material is reachable from where you actually are. It is a compiler with a grammar and a vocabulary as its build inputs, not a conversation partner improvising at whatever level it guesses you are at. The voicing of that material comes from a separate TTS provider.

## Coverage as the honest progress metric

The reader measures how much of a real text you already know, word by word and structure by structure, and reports that coverage rather than a score. A text you cover at ninety percent is close to comprehensible input; a text you cover at forty percent is not yet worth your time. Coverage answers "am I ready for this" with a number instead of a guess, and it is the same number that tells you what to learn next in order to raise it.
