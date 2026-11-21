# 0015 · An audio lesson is today's queue said Pimsleur's way, planned in the core, spliced by ffmpeg, and graded afterwards

Date: 2026-11-21. Status: accepted.

## Context

v0.12.0 turns the day's queue into an audio lesson for a walk: Pimsleur's template - a question, a pause to answer aloud, the answer, a repeat - with each phrase coming back inside the lesson at growing intervals. The voice (ADR of v0.3, `src/voice/`) already says every sentence through a cache; Today already knows what is due. Each part of the lesson has a cheap version a later one would undo.

**Where the plan lives.** The plan - which lines, in what order, with how much silence, which phrases fit - is arithmetic. Written inside the server's lesson builder, it is tested only through a database, a voice and ffmpeg, and its rules (the pauses, the intervals, the length) would be checked by listening.

**What the lesson is made of.** A lesson could be its own material - new sentences, a queue of its own - or the samples of every formula started, as the old Listen mode played. Then the day would have two queues, and what was heard would not be what was due.

**How the sound is put together.** ffmpeg's concat filter over hundreds of inputs and silences is one command, but its timings are the filter's to decide, and chapters placed by summed durations drift from the file a little per line.

**What a heard phrase does to its card.** Heard is not answered: the learner said something into the wind. Counting every phrase played as *good* moves the schedule on something nobody checked; not counting it at all makes the walk a second lesson the schedule does not know about.

**What a lesson costs.** Every new line in the language being learnt costs ElevenLabs credits. A lesson that runs out of them halfway fails having spent them.

## Decision

**The plan is a pure function in `hilvan-core`** (`lesson::plan`). An asking is the question, a pause of three seconds plus the phrase's length (four to six), the answer, room to say the phrase (its length plus a second, 1.5 to 6), and the phrase again in the native voice. A phrase never met is first heard - a new formula's explanation, the meaning, the phrase slowly, the phrase at speed - and asked about 20 s, 2 min and 8 min after; one already met is asked at once and again 5 min on. A phrase comes in only when every time it will be heard fits the length; a recall that falls due is asked first; when nothing is left to come in, the soonest recall is asked early rather than leaving silence. Mutations of the intervals, the pause bounds, the length and the priority of recalls each fail a test.

**The lesson is the queue.** Today's queue in its order - due, then new - then the cards coming back within seven days, soonest first. A formula is heard in one of its samples, another each day; a word in its anchor sentence with its translation, or alone with its gloss when the sentence is a text's. Pairs are left to the cards: a choice of word for a gap is done with the eyes. The old Listen mode and `GET /api/listen` are removed: one way to hear the day.

**The sound is laid out sample by sample.** Each line is said through the cache and decoded once by ffmpeg to 16-bit mono at 24 kHz, cut to whole milliseconds; the server writes the lines and the silences in order and ffmpeg encodes the whole once - MP3, 48 kbit/s - with a chapter per stretch from an ffmetadata file, written as ID3 `CHAP` frames. The plan is made twice: on what the cache knows and a guess for the rest, to choose the phrases and price them; then on the decoded lengths, so every chapter starts where the file does. ffmpeg is a process the server runs (`HILVAN_FFMPEG`); the image installs Debian's.

**A lesson is priced before it is made.** The new lines in a language ElevenLabs speaks are counted at the plan's rate. More than the month has left, and the lesson is not started, saying both numbers; any cost at all while the account cannot be asked, and it is not started either.

**Heard is graded afterwards.** When the lesson ends, or the learner stops, every phrase asked to the end is listed held; the learner taps those that slipped. Each is an answer through the device's line (ADR 0011) - `POST /api/heard`, the card named in the body, taken at the moment it was heard - recorded as an asked review marked `heard`, untimed and with nothing typed. A phrase only heard for the first time and never asked is not answered.

**A lesson is kept like a sound.** In the database, with its phrases (each naming its card) and its chapters; the three latest are kept. It is made in the background, one at a time; one left half made by a restart is marked failed at the next start. The app downloads the file whole into a cache of its own - not the app's, which a new version clears - keeps only the latest, and plays it from the device with the lock screen's buttons skipping by phrase.

**The learner model file is format 6.** Every review carries `heard`; the file carries `lesson_minutes`, the length the learner asks for. Format 5 is refused.

## Consequences

- A lesson says only what the packs hold. New sentences for it are the compiler's, a later version's; the template is the same.
- The first lesson of a pack is slow on a Pi: every line is new to the cache, one to two seconds each for Piper. Later ones are made of what has been said and take seconds.
- What a lesson heard is graded by the learner, held or slipped - not again, hard, good and easy: a walk is no place to weigh four grades, and the cards keep them.
- A stand without ffmpeg drills and reads as before; asking for a lesson fails and says why, and the server says so at startup.
- CI installs ffmpeg on both platforms: the lesson tests run the real one.
