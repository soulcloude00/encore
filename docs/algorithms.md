# Recommendation and playback paths

Recommendations are local, deterministic ranking heuristics over YouTube Music
candidate lists. No collaborative model or remote profile service is used.

## Feedback

A natural ending or listening past 70% counts as complete. Moving on before 30 seconds
and before half the track counts as a skip; other listens are partial. Invalid or
negative timing samples do not become skip feedback. A track that never started does
not teach a negative preference. Queue replacement records the outgoing track before
replacing its backing list, so feedback stays attached to the right song.

Likes contribute 3 points. Each completion contributes 1, each skip subtracts 1.2,
and each play contributes 0.1. Historical scores decay with a two-week half-life
from the track's most recent play. This is an aggregate approximation: replaying a
track refreshes its aggregate, rather than keeping a per-event decay history.

## Candidate ranking

1. Drop avoided IDs, repeatedly skipped/uncompleted tracks without a like, and filler.
2. Give position `p` in a source of length `n` relevance `1 - p/n`.
3. Count at most one vote per track per source; sum across independent sources.
4. Add artist affinity once, scaled by 0.15 and bounded to [-0.6, 0.35].
5. Sort descending by score, breaking ties by track ID.
6. Collapse alternate uploads with the same normalized title and primary artist.
   Version words such as live/remix remain part of the title.
7. Space artists over a three-song lookback when alternatives exist, then apply the limit.

Artist grouping normalizes case and common collaboration separators. It is a heuristic,
not an artist identity database. Title matching may merge different recordings with
the exact same title; it deliberately retains version qualifiers.

## Stations

The seed anchors a station. Refills alternate between that seed and the latest
completed song. Upcoming queue IDs and 40 recent history IDs are excluded. Two skips
of an artist ban that artist for the session, except the seed artist. Bans are applied
before the result limit so they cannot consume the entire candidate budget.

New artists are spaced against the immutable existing queue tail. Old queue entries
never move during refills. A new station invalidates outstanding requests through an
epoch. Errors and empty results clear pending state, retry once, then stop automatic
requests and show a message. Starting another station resets the retry budget.

## Verification

Synthetic unit tests cover feedback thresholds, invalid samples, dislike/avoid
filtering, overlapping sources, duplicate source votes, alternate uploads, stable
ties, zero limits, artist grouping, and spacing against a queued tail. Station tests
cover stale delivery and exhausted retry budgets. Live service quality still requires
manual listening: these checks prove behavior, not recommendation satisfaction.
