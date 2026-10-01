"""Builds the English lexicon hilvan ships: lemmas ranked by how often they
are met, with their inflected forms and a transcription.

    pip install wordfreq==3.1.1
    python tools/lexicon/build.py        # writes lexicons/en/lexicon.tsv

The other sources are downloaded at pinned commits into tools/lexicon/.cache.
See tools/lexicon/README.md for what each one contributes and under which
licence, and why these and not others.
"""

from __future__ import annotations

import argparse
import collections
import dataclasses
import pathlib
import re
import sys
import urllib.request

import wordfreq

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
CACHE = HERE / ".cache"

WORDFREQ_VERSION = "3.1.1"
ESDB_COMMIT = "1e5b7d3a72f47a71da5d28686c1dd4b397178485"
ESDB_URL = f"https://raw.githubusercontent.com/en-wl/wordlist/{ESDB_COMMIT}/data/scowl-pre.txt"
EWT_TAG = "r2.18"
EWT_URL = "https://raw.githubusercontent.com/UniversalDependencies/UD_English-EWT/{tag}/en_ewt-ud-{part}.conllu"
CMUDICT_COMMIT = "74790861f652b15e4ac49015a90074ad62a27690"
CMUDICT_URL = f"https://raw.githubusercontent.com/cmusphinx/cmudict/{CMUDICT_COMMIT}/cmudict.dict"

# The largest SCOWL size a word may come from: 70 is "large", the size of a
# good desk dictionary. 80 and 95 are scrabble and specialist lists.
MAX_LEVEL = 70

# Parts of speech that make a lemma: noun, verb, adjective, adverb, pronoun,
# preposition, conjunction, article and determiner, interjection, and the
# multi-part verbs. Names, places, abbreviations, affixes, contractions and
# numerals do not.
WORD_TAGS = {"n", "v", "aj", "av", "pn", "pp", "c", "a", "i", "m"}

# Nationalities and languages are spelt with a capital and are words all the
# same: "English", "Russian".
CAPITAL_TAGS = WORD_TAGS | {"n/demonym", "aj/demonym"}

# Spellings a frequency list counts that are not the word they look like: the
# rest of a contraction some of its sources split ("don" of "don't").
NOT_WORDS = {"don"}

# How many times a form has to be seen in the treebank before what the
# treebank says about it is trusted over a guess.
TREEBANK_MIN = 3

LOWER = re.compile(r"^[a-z]+(?:[-'][a-z]+)*$")
CAPITAL = re.compile(r"^[A-Z][a-z]+$")
ENTRY = re.compile(r"(\S+(?: \S+)*?) <([^>]*)>(?: \{[^}]*\})?(?:: (.*))?$")


def fetch(url: str, name: str) -> pathlib.Path:
    CACHE.mkdir(exist_ok=True)
    path = CACHE / name
    if not path.exists():
        print(f"downloading {url}", file=sys.stderr)
        with urllib.request.urlopen(url) as response:
            path.write_bytes(response.read())
    return path


@dataclasses.dataclass
class Lemma:
    """One headword: how it is written, and every spelling it takes."""

    spelling: str
    level: int
    forms: set[str] = dataclasses.field(default_factory=set)


def parse_esdb(path: pathlib.Path) -> dict[str, Lemma]:
    """Lemmas by their lowercase key, from the English Speller Database.

    A block is a headword and its lines; a line whose headword is `-` adds
    forms at another size. Kept: dictionary words at size 70 or below in the
    American spelling (the voices and the transcription are American, and a
    lemma has one spelling), and their forms at the same sizes.
    """
    lemmas: dict[str, Lemma] = {}
    current: Lemma | None = None
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.split(" #", 1)[0].rstrip()
        if not line:
            current = None
            continue
        if ": " not in line:
            continue
        levels, entry = line.split(": ", 1)
        level_match = re.match(r"\s*(\d+)", levels)
        if not level_match:
            continue
        level = int(level_match.group(1))
        variant, head_part = "", entry
        lt, colon = entry.find("<"), entry.find(": ")
        if colon != -1 and (lt == -1 or colon < lt):
            variant, head_part = entry[:colon], entry[colon + 2 :]
        match = ENTRY.match(head_part)
        if not match:
            continue
        head, tag, forms = match.group(1), match.group(2), match.group(3) or ""
        if head != "-":
            current = None
            if level > MAX_LEVEL or (variant and not american(variant)):
                continue
            if LOWER.match(head) and tag in WORD_TAGS:
                pass
            elif CAPITAL.match(head) and tag in CAPITAL_TAGS:
                pass
            else:
                continue
            key = head.lower()
            if (len(key) == 1 and key != "a") or key in NOT_WORDS:
                continue
            lemma = lemmas.get(key)
            if lemma is None:
                lemma = lemmas[key] = Lemma(head, level, {key})
            elif level < lemma.level or (level == lemma.level and head == key):
                # One key, two spellings - "god" and "God", "english" the
                # verb and "English": the commoner wins, lowercase on a tie.
                lemma.spelling, lemma.level = head, level
            current = lemma
        elif current is None or level > MAX_LEVEL:
            continue
        for item in split_forms(forms):
            form = first_alternative(item)
            if form and LOWER.match(form.lower()) and "'" not in form:
                current.forms.add(form.lower())
    # The pronoun is spelt in capitals, and the database files it as a letter.
    lemmas["i"] = Lemma("I", 10, {"i"})
    return lemmas


def american(variant: str) -> bool:
    """Whether a spelling-variant prefix - "A B", "AV", "B C", "A. B. C" -
    marks the American spelling of choice rather than a lesser variant."""
    return any(tag in ("A", "A=", "A.") for tag in variant.split())


def split_forms(forms: str) -> list[str]:
    items, depth, start = [], 0, 0
    for i, ch in enumerate(forms):
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        elif ch == "," and depth == 0:
            items.append(forms[start:i].strip())
            start = i + 1
    items.append(forms[start:].strip())
    return [item for item in items if item]


def first_alternative(item: str) -> str | None:
    """The preferred spelling of one form: "(was | @: wast)" is "was"."""
    first = item.strip("() ").split("|", 1)[0].strip()
    if ":" in first:
        return None
    first = first.rstrip("~-!?")
    return None if first in ("", "-") else first


@dataclasses.dataclass
class Treebank:
    """What a corpus annotated by hand says about a spelling: which lemma
    stood behind it each time, and how often it was a name."""

    lemmas: dict[str, collections.Counter[str]]
    names: collections.Counter[str]
    seen: collections.Counter[str]

    def name_share(self, form: str) -> float:
        total = self.seen[form]
        return self.names[form] / total if total >= TREEBANK_MIN else 0.0


def parse_treebank(paths: list[pathlib.Path]) -> Treebank:
    lemmas: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    names: collections.Counter[str] = collections.Counter()
    seen: collections.Counter[str] = collections.Counter()
    for path in paths:
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line or line.startswith("#"):
                continue
            columns = line.split("\t")
            if len(columns) < 4 or not columns[0].isdigit() or columns[3] in ("PUNCT", "SYM", "X", "NUM"):
                continue
            form = columns[1].lower()
            seen[form] += 1
            if columns[3] == "PROPN":
                names[form] += 1
            else:
                lemmas[form][columns[2].lower()] += 1
    return Treebank(lemmas, names, seen)


def split(form: str, candidates: set[str], treebank: Treebank, evidence: dict[str, float]) -> dict[str, float]:
    """How a spelling claimed by several lemmas shares out its frequency.

    A frequency list counts spellings, and "left" is both the direction and
    the past of "leave". The treebank knows which word each one was, so its
    split is used when it has seen the form often enough. Otherwise each
    lemma gets a share in proportion to how often it is met in spellings
    nobody else claims - which folds a participle into its verb and leaves a
    noun with no plural in use, like "found", nothing.
    """
    counts = treebank.lemmas.get(form, collections.Counter())
    tagged = {lemma: counts.get(lemma, 0) for lemma in candidates}
    total = sum(tagged.values())
    if total >= TREEBANK_MIN:
        smoothed = {lemma: n + 0.25 for lemma, n in tagged.items()}
        whole = sum(smoothed.values())
        return {lemma: n / whole for lemma, n in smoothed.items()}
    weights = {lemma: evidence[lemma] for lemma in candidates}
    whole = sum(weights.values())
    if whole == 0:
        return {lemma: 1 / len(candidates) for lemma in candidates}
    return {lemma: weight / whole for lemma, weight in weights.items()}


ARPABET = {
    "AA": "ɑ", "AE": "æ", "AH": "ʌ", "AO": "ɔ", "AW": "aʊ", "AY": "aɪ", "EH": "ɛ", "ER": "ɝ",
    "EY": "eɪ", "IH": "ɪ", "IY": "i", "OW": "oʊ", "OY": "ɔɪ", "UH": "ʊ", "UW": "u",
    "B": "b", "CH": "tʃ", "D": "d", "DH": "ð", "F": "f", "G": "ɡ", "HH": "h", "JH": "dʒ",
    "K": "k", "L": "l", "M": "m", "N": "n", "NG": "ŋ", "P": "p", "R": "ɹ", "S": "s", "SH": "ʃ",
    "T": "t", "TH": "θ", "V": "v", "W": "w", "Y": "j", "Z": "z", "ZH": "ʒ",
}

# Consonant clusters English allows at the start of a syllable: the stress
# mark goes in front of the longest of them before a stressed vowel.
ONSETS = {
    tuple(onset.split())
    for onset in (
        "P R", "T R", "K R", "B R", "D R", "G R", "F R", "TH R", "SH R", "P L", "K L", "B L",
        "G L", "F L", "S L", "S M", "S N", "S W", "T W", "K W", "D W", "TH W", "G W", "S P",
        "S T", "S K", "S F", "P Y", "B Y", "K Y", "F Y", "M Y", "HH Y", "V Y", "S P R", "S T R",
        "S K R", "S P L", "S K W",
    )
}


def to_ipa(phones: list[str]) -> str:
    """ARPAbet to IPA, the stress marks in front of their syllables.

    A word of one syllable carries no mark: there is nothing to tell it from.
    """
    syllables = sum(1 for phone in phones if phone[-1:] in "012")
    out: list[str] = []
    pending: list[str] = []
    for phone in phones:
        base = phone.rstrip("012")
        stress = phone[len(base) :]
        if not stress:
            pending.append(base)
            continue
        onset = 0
        for n in range(min(3, len(pending)), 0, -1):
            if n == 1 or tuple(pending[-n:]) in ONSETS:
                onset = n
                break
        out.extend(ARPABET[p] for p in pending[: len(pending) - onset])
        if syllables > 1 and stress in "12":
            out.append("ˈ" if stress == "1" else "ˌ")
        out.extend(ARPABET[p] for p in pending[len(pending) - onset :])
        if base == "AH" and stress == "0":
            out.append("ə")
        elif base == "ER" and stress == "0":
            out.append("ɚ")
        else:
            out.append(ARPABET[base])
        pending = []
    out.extend(ARPABET[p] for p in pending)
    return "".join(out)


def parse_cmudict(path: pathlib.Path) -> dict[str, str]:
    ipa: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.split(" #", 1)[0].strip()
        if not line:
            continue
        word, *phones = line.split()
        if "(" in word:
            continue  # alternative pronunciations: the first one is the common one
        ipa.setdefault(word.lower(), to_ipa(phones))
    return ipa


HEADER = """\
# The English lexicon of hilvan: lemmas by how often they are met.
#
# rank <TAB> lemma <TAB> transcription (IPA, American) <TAB> other forms, space-separated
#
# Generated by tools/lexicon/build.py - do not edit by hand; change the
# builder and run it again. Built from:
#   wordfreq {wordfreq} (Robyn Speer), word frequencies - CC BY-SA 4.0
#   English Speller Database / SCOWL v2 at en-wl/wordlist@{esdb} (Kevin Atkinson), lemmas and forms - MIT-like
#   UD English-EWT {ewt} (Stanford), which lemma a spelling was - CC BY-SA 4.0
#   CMU Pronouncing Dictionary at cmusphinx/cmudict@{cmudict} (Carnegie Mellon University), transcriptions - BSD-2-Clause
#
# This file is licensed CC BY-SA 4.0. See lexicons/en/README.md.
"""


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=str(ROOT / "lexicons/en/lexicon.tsv"))
    parser.add_argument("--size", type=int, default=10_000)
    parser.add_argument("--report", action="store_true", help="print how the commonest shared spellings were split, and stop")
    args = parser.parse_args()

    if wordfreq.__dict__.get("__version__", WORDFREQ_VERSION) != WORDFREQ_VERSION:
        sys.exit(f"wordfreq {WORDFREQ_VERSION} is what this lexicon is built from")
    lemmas = parse_esdb(fetch(ESDB_URL, f"scowl-pre-{ESDB_COMMIT[:12]}.txt"))
    treebank = parse_treebank(
        [fetch(EWT_URL.format(tag=EWT_TAG, part=part), f"en_ewt-{EWT_TAG}-{part}.conllu") for part in ("train", "dev", "test")]
    )
    ipa = parse_cmudict(fetch(CMUDICT_URL, f"cmudict-{CMUDICT_COMMIT[:12]}.dict"))

    claims: dict[str, set[str]] = collections.defaultdict(set)
    for key, lemma in lemmas.items():
        for form in lemma.forms:
            claims[form].add(key)

    freq = wordfreq.get_frequency_dict("en")
    evidence: dict[str, float] = collections.defaultdict(float)
    for form, owners in claims.items():
        if len(owners) == 1:
            evidence[next(iter(owners))] += freq.get(form, 0.0)

    totals: dict[str, float] = collections.defaultdict(float)
    report = []
    for form, f in freq.items():
        candidates = claims.get(form)
        if not candidates:
            continue
        # A spelling that is mostly somebody's name - "john", "china" - is
        # counted for the share of the time it is not. Not for a word whose
        # own spelling is capitalised: "English" the language is a name too.
        if all(lemmas[key].spelling.islower() for key in candidates):
            f *= 1 - treebank.name_share(form)
        if len(candidates) == 1:
            totals[next(iter(candidates))] += f
            continue
        shares = split(form, candidates, treebank, evidence)
        for key, share in shares.items():
            totals[key] += f * share
        report.append((f, form, {key: round(share, 2) for key, share in shares.items()}))

    if args.report:
        report.sort(reverse=True)
        for f, form, shares in report[:150]:
            print(f"{form:12} {wordfreq.zipf_frequency(form, 'en'):5.2f} {shares}")
        return

    ranked = sorted(totals.items(), key=lambda item: (-item[1], item[0]))[: args.size]
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8", newline="\n") as file:
        file.write(HEADER.format(wordfreq=WORDFREQ_VERSION, esdb=ESDB_COMMIT[:12], ewt=EWT_TAG, cmudict=CMUDICT_COMMIT[:12]))
        for rank, (key, _) in enumerate(ranked, start=1):
            lemma = lemmas[key]
            forms = " ".join(sorted(lemma.forms - {key}))
            file.write(f"{rank}\t{lemma.spelling}\t{ipa.get(key, '')}\t{forms}\n")
    running = sum(freq.values())
    for band in (1000, 2000, 5000):
        print(f"the top {band} lemmas carry {sum(f for _, f in ranked[:band]) / running:.1%} of running text", file=sys.stderr)
    print(f"wrote {len(ranked)} lemmas to {out}", file=sys.stderr)


if __name__ == "__main__":
    main()
