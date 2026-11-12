-- Confusion pairs and topics.
--
-- A turn keeps the words typed in the place of others, read into lemmas:
-- [["tell", "say"]] when "said" stood where "told" should have. Two lemmas
-- swapped often enough become a card of kind 'pair', subject "en:say|tell"
-- - the two in order - and the card holds no row of its own: what the pair
-- is, is what the turns say.
--
-- A pack names the topics its sentences are about, and each sentence the
-- topics it is in, so a sitting can be of one topic.

ALTER TABLE attempt ADD COLUMN swapped TEXT NOT NULL DEFAULT '[]';   -- JSON array of [expected lemma, typed lemma]

CREATE TABLE topic (
    pack_id  TEXT NOT NULL REFERENCES pack(id) ON DELETE CASCADE,
    id       TEXT NOT NULL,      -- from the pack: "food"
    name     TEXT NOT NULL,      -- as the app shows it: "Food"
    position INTEGER NOT NULL,   -- the order the pack lists them in
    PRIMARY KEY (pack_id, id)
) STRICT;

CREATE TABLE sentence_topic (
    sentence_id INTEGER NOT NULL REFERENCES sentence(id) ON DELETE CASCADE,
    pack_id     TEXT NOT NULL,
    topic_id    TEXT NOT NULL,
    PRIMARY KEY (sentence_id, pack_id, topic_id),
    FOREIGN KEY (pack_id, topic_id) REFERENCES topic(pack_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX sentence_topic_by_topic ON sentence_topic(pack_id, topic_id);
