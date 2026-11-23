//! Blender's search, as its menus have it: what is typed is cut into words,
//! and each must find a word of the label of its own, in any order -- the
//! whole word, its beginning, a piece inside it, the initials of a run of
//! words, or the word with a typo or two. "ad cu" finds "Add cube", "shdow"
//! finds `shadows`, "lp" finds `lod_pixels`.
//!
//! Labels are cut at anything not a letter or a digit and where camelCase
//! starts a word, so setting names (`shading.ambient`), choices
//! (`CatppuccinMocha`) and paths (`examples/sphere/tpm.py`) read as the words
//! they are made of. A match scores by how well each word was found, with a
//! bonus for words found in the order typed and for a label that begins with
//! the first; the label's length breaks ties.

use std::cmp::Reverse;

/// What one query word's match is worth, by kind, best first. A near miss
/// is worth `FUZZY` with one typo and half that with two.
const EQUAL: u32 = 100;
const PREFIX: u32 = 80;
const INITIALS: u32 = 60;
const CONTAINS: u32 = 40;
const FUZZY: u32 = 20;
/// For each query word found after the one typed before it.
const IN_ORDER: u32 = 10;
/// And more for one found right after it: "lod pix" in `lod_pixels` over
/// `lod_max_pixels`.
const ADJACENT: u32 = 5;
/// For a label that begins with the query's first word: "amb" in `ambient`
/// over `shading.ambient`.
const FIRST_WORD: u32 = 10;
// The search's bound allows each word a later word's bonuses at most, the
// first word's included.
const _: () = assert!(FIRST_WORD <= IN_ORDER + ADJACENT);
/// The most ways of placing the query's words a label is searched for:
/// a handful for anything typed in earnest, and a bound on a frame's time
/// for a query of many one-letter words, each found in every word. Past it,
/// the best placing found so far stands.
const BUDGET: u32 = 1000;

/// How well `query` matches `candidate`, higher better; `None` when a word
/// of the query finds no word of the candidate left to it. An empty query
/// matches anything, all equally.
pub fn score(query: &str, candidate: &str) -> Option<u32> {
    score_words(&words(query), candidate)
}

/// The items whose labels match `query`, best first. Equal scores keep the
/// order the items came in, so an empty query leaves the list as it was.
pub fn rank<T, S>(query: &str, items: impl IntoIterator<Item = (T, S)>) -> Vec<T>
where
    S: AsRef<str>,
{
    let query = words(query);
    let mut found: Vec<(u32, T)> =
        items.into_iter().filter_map(|(item, label)| Some((score_words(&query, label.as_ref())?, item))).collect();
    found.sort_by_key(|(score, _)| Reverse(*score)); // stable: ties keep their order
    found.into_iter().map(|(_, item)| item).collect()
}

/// `score`, with the query already cut into words: `rank` cuts it once.
fn score_words(query: &[Vec<char>], candidate: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let words = words(candidate);
    if query.len() > words.len() {
        return None;
    }
    let hits: Vec<Vec<Hit>> = query.iter().map(|q| hits(q, &words)).collect();
    // The most the query's words from each on can add, bonuses included;
    // a word found nowhere ends it here.
    let mut bound = vec![0; query.len() + 1];
    for i in (0..query.len()).rev() {
        bound[i] = bound[i + 1] + hits[i].first()?.points + IN_ORDER + ADJACENT;
    }
    let (mut used, mut budget) = (vec![false; words.len()], BUDGET);
    let points = place(&hits, &bound, 0, None, &mut used, &mut budget)?;
    // The label's length breaks ties, under every point of the match.
    let shortness = 255 - candidate.chars().count().min(255) as u32;
    Some(points.saturating_mul(256).saturating_add(shortness))
}

/// One way a query word matches: the label's words `start..end`, more than
/// one only for initials.
#[derive(Clone, Copy)]
struct Hit {
    start: usize,
    end: usize,
    points: u32,
}

/// Every way query word `q` matches among `words`, best first and, among
/// equals, the earlier word first.
fn hits(q: &[char], words: &[Vec<char>]) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (j, w) in words.iter().enumerate() {
        if let Some(points) = matches(q, w) {
            hits.push(Hit { start: j, end: j + 1, points });
        }
        if let Some(n) = initials(q, &words[j..]).filter(|&n| n > 1) {
            hits.push(Hit { start: j, end: j + n, points: INITIALS });
        }
    }
    hits.sort_by_key(|hit| Reverse(hit.points));
    hits
}

/// The best the query's words from `i` on can score, each on words of the
/// label none of the others took (`used`), the one before found at `before`.
/// Tried best hit first, so that the first placing found is usually the
/// best and the bound cuts the rest short.
fn place(
    hits: &[Vec<Hit>],
    bound: &[u32],
    i: usize,
    before: Option<Hit>,
    used: &mut [bool],
    budget: &mut u32,
) -> Option<u32> {
    let Some(options) = hits.get(i) else { return Some(0) };
    let mut best: Option<u32> = None;
    for &hit in options {
        if *budget == 0 || best.is_some_and(|best| hit.points + IN_ORDER + ADJACENT + bound[i + 1] <= best) {
            break;
        }
        if used[hit.start..hit.end].contains(&true) {
            continue;
        }
        *budget -= 1;
        let bonus = match before {
            None if hit.start == 0 => FIRST_WORD,
            Some(before) if hit.start == before.end => IN_ORDER + ADJACENT,
            Some(before) if hit.start > before.start => IN_ORDER,
            _ => 0,
        };
        used[hit.start..hit.end].fill(true);
        let rest = place(hits, bound, i + 1, Some(hit), used, budget);
        used[hit.start..hit.end].fill(false);
        best = best.max(rest.map(|rest| hit.points + bonus + rest));
    }
    best
}

/// How query word `q` matches word `w` on its own, if it does.
fn matches(q: &[char], w: &[char]) -> Option<u32> {
    if q == w {
        Some(EQUAL)
    } else if w.starts_with(q) {
        Some(PREFIX)
    } else if w.windows(q.len()).any(|piece| piece == q) {
        Some(CONTAINS)
    } else {
        // No typo in a word of one or two letters, which could be any word
        // of the same length with one; one from three letters, two from seven.
        let allowed = match q.len() {
            0..3 => return None,
            3..7 => 1,
            _ => 2,
        };
        if w.len() + allowed < q.len() {
            return None; // too short to come that near, even whole
        }
        // Never 0: that would be `w` beginning with `q`, a prefix.
        let typos = typos(q, w);
        (typos <= allowed).then(|| FUZZY / typos as u32)
    }
}

/// The fewest edits -- a letter added, dropped or changed, or two side by
/// side swapped -- that make `q` the beginning of `w`, or `w` whole:
/// "shdow" is one from `shadows`, "ambeint" one from `ambient`, "exprt" one
/// from `export`. Against a beginning as well as the word, as what is typed
/// is usually a word not yet finished.
fn typos(q: &[char], w: &[char]) -> usize {
    // d[i][j]: the edits between q[..i] and w[..j], each letter edited once
    // at most (the optimal string alignment distance).
    let mut d = vec![vec![0; w.len() + 1]; q.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=q.len() {
        for j in 1..=w.len() {
            let mut edits =
                (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + usize::from(q[i - 1] != w[j - 1]));
            if i > 1 && j > 1 && q[i - 1] == w[j - 2] && q[i - 2] == w[j - 1] {
                edits = edits.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = edits;
        }
    }
    d[q.len()].iter().copied().min().unwrap_or(0)
}

/// How many words, from the first of `words` on, `q` spells in turn, the
/// fewest that do: each word giving its initial or the whole of it, the
/// last any beginning: "lp", "lpix" and "lodpix" spell two of `lod_pixels`,
/// "lopi" none, its "lo" being neither.
fn initials(q: &[char], words: &[Vec<char>]) -> Option<usize> {
    if words.first()?.first() != q.first() {
        return None;
    }
    // reached[k]: q[..k] spelled by the words so far.
    let mut reached = vec![false; q.len() + 1];
    reached[0] = true;
    for (n, w) in words.iter().enumerate() {
        let mut next = vec![false; q.len() + 1];
        for k in (0..q.len()).filter(|&k| reached[k]) {
            let common = q[k..].iter().zip(w).take_while(|(a, b)| a == b).count();
            if common > 0 {
                next[k + 1] = true;
                if common == w.len() || k + common == q.len() {
                    next[k + common] = true;
                }
            }
        }
        if next[q.len()] {
            return Some(n + 1);
        }
        if !next.contains(&true) {
            return None;
        }
        reached = next;
    }
    None
}

/// `text` in lower case, cut into words: at anything not a letter or a
/// digit, and where camelCase starts one -- `CatppuccinMocha` is
/// "catppuccin mocha", `HTTPServer` "http server".
fn words(text: &str) -> Vec<Vec<char>> {
    let chars: Vec<char> = text.chars().collect();
    let lower = |c: Option<char>| c.is_some_and(char::is_lowercase);
    let upper = |c: Option<char>| c.is_some_and(char::is_uppercase);
    let mut words = Vec::new();
    let mut word = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        let (before, after) = (i.checked_sub(1).map(|i| chars[i]), chars.get(i + 1).copied());
        // A capital after a small letter starts a word, and so does the last
        // of a run of capitals before a small one: "HTTPServer".
        let camel = c.is_uppercase() && (lower(before) || (upper(before) && lower(after)));
        if (camel || !c.is_alphanumeric()) && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        if c.is_alphanumeric() {
            word.extend(c.to_lowercase());
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranked(query: &str, labels: &[&'static str]) -> Vec<&'static str> {
        rank(query, labels.iter().map(|&label| (label, label)))
    }

    #[test]
    fn words_match_in_any_order() {
        let in_order = score("ad cu", "Add cube").expect("both words begin words of the label");
        let reversed = score("cu ad", "Add cube").expect("in either order");
        assert!(in_order > reversed, "as typed scores higher: {in_order} against {reversed}");
    }

    #[test]
    fn a_typo_or_two_is_forgiven() {
        assert!(score("shdow", "shadows").is_some(), "a letter dropped, against the word's beginning");
        assert!(score("ambeint", "ambient").is_some(), "two letters swapped");
        assert!(score("exprt frame", "Export frame").is_some());
        assert!(score("ambeint", "ambient") < score("ambient", "ambient"));
    }

    /// One or two letters with a typo could be almost any short word.
    #[test]
    fn short_words_are_taken_as_typed() {
        assert_eq!(score("ax", "Add cube"), None);
        assert_eq!(score("xyz", "Add cube"), None);
    }

    /// Each query word takes a word of the label of its own: one label word
    /// cannot serve two query words, nor initials share theirs.
    #[test]
    fn every_query_word_needs_a_word_of_its_own() {
        assert_eq!(score("cube cube", "Add cube"), None);
        assert_eq!(score("ad ad", "Add cube"), None);
        assert!(score("cube cube", "cube on cube").is_some());
        assert_eq!(score("lp pix", "lod_pixels"), None, "\"lp\" took both words");
        assert_eq!(score("add cube now", "Add cube"), None, "more words than the label has");
    }

    #[test]
    fn lod_pix_finds_pixels_not_texels() {
        assert!(score("lod pix", "lod_pixels").is_some());
        assert_eq!(score("lod pix", "lod_texels"), None);
    }

    #[test]
    fn the_beginning_of_a_word_beats_its_inside() {
        assert_eq!(ranked("amb", &["camber", "shading.ambient", "ambient"]), ["ambient", "shading.ambient", "camber"]);
        let sha = ranked("sha", &["flash", "shading", "shadows"]);
        assert_eq!(sha[..2], ["shading", "shadows"]);
    }

    #[test]
    fn initials_find_a_run_of_words() {
        assert!(score("lp", "lod_pixels").is_some());
        assert!(score("lodpix", "lod_pixels").is_some(), "a name typed without its underscore");
        assert!(score("lpix", "lod_pixels").is_some());
        assert_eq!(score("lopi", "lod_pixels"), None, "\"lo\" is neither an initial nor a word");
        assert!(score("catppuccinmocha", "CatppuccinMocha").is_some());
        assert_eq!(ranked("lp", &["lod_pixels", "lp_norm"]), ["lp_norm", "lod_pixels"]);
    }

    #[test]
    fn labels_are_cut_into_their_words() {
        assert!(score("mocha", "CatppuccinMocha").is_some(), "camelCase");
        assert!(score("server", "HTTPServer").is_some(), "an acronym before a word");
        assert!(score("tpm sph", "examples/sphere/tpm.py").is_some(), "a path");
        assert!(score("ambient", "shading.ambient").is_some());
        assert_eq!(score("ADD CUBE", "add cube"), score("add cube", "Add cube"), "case is ignored");
    }

    #[test]
    fn a_shorter_label_wins_a_tie() {
        assert_eq!(ranked("amb", &["ambient_occlusion", "ambient"]), ["ambient", "ambient_occlusion"]);
    }

    #[test]
    fn equal_scores_keep_the_input_order() {
        assert_eq!(ranked("sha", &["shadows", "shading"]), ["shadows", "shading"]);
        assert_eq!(ranked("sha", &["shading", "shadows"]), ["shading", "shadows"]);
    }

    #[test]
    fn an_empty_query_keeps_the_order() {
        let labels = ["Play", "Step", "Export frame", "update_betas"];
        assert_eq!(ranked("", &labels), labels);
        assert_eq!(ranked("  ", &labels), labels);
        assert_eq!(score("", "anything"), score("", ""));
    }

    /// Eight one-letter words, each found in every word of a long label:
    /// millions of placings, of which the first tried is already the best.
    #[test]
    fn many_short_words_stay_quick() {
        let label = ["aa"; 16].join(" ");
        assert!(score("a a a a a a a a", &label).is_some());
        assert_eq!(score("a a a a a a a a b", &label), None);
    }
}
