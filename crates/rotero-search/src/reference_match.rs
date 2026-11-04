//! Pick which of a paper's known references a bibliography entry is.
//!
//! Hand-parsing a PDF reference into title/authors is fragile: words wrap
//! between lines, columns interleave, hyphens split words. But once we hold the
//! citing paper's reference list (see [`crate::openalex::fetch_references`]),
//! the question shrinks from "which paper in the world is this?" to "which of
//! these ~50 titles appears in this text?" — answered from the raw entry text
//! with no parse at all.

use rotero_models::Paper;

/// Minimum share of a title's significant words found, in order, in the entry.
const MIN_SCORE: f64 = 0.8;
/// How far ahead of the runner-up the winner must be.
const MIN_MARGIN: f64 = 0.15;

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "into", "via", "using", "are", "its", "our", "that",
    "this", "of", "on", "in", "to", "a", "an", "by", "at", "or", "is",
];

/// Index of the candidate whose title is in `entry`, if exactly one clearly is.
///
/// Scores each title by the longest in-order run of its significant words
/// found in the entry (so *Pattern Recognition and Neural Networks* does not
/// match an entry for *Neural Networks for Pattern Recognition*), tolerating a
/// stray word or two from line-wrapping. Accepts the best only when it covers
/// at least [`MIN_SCORE`] of its title and clearly beats every different title.
pub fn match_reference(entry: &str, candidates: &[Paper]) -> Option<usize> {
    let entry_forms = [words(entry, false), words(entry, true)];

    let mut scored: Vec<(usize, f64, usize)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let (score, matched) = [false, true]
                .into_iter()
                .flat_map(|join| {
                    let title = significant(&words(&p.title, join));
                    entry_forms
                        .iter()
                        .map(move |e| title_score(&title, e))
                        .collect::<Vec<_>>()
                })
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;
            Some((i, score, matched))
        })
        .collect();
    // Higher score first; among equals, the title that matched more words
    // (so "Caffe: Convolutional architecture…" beats a bare "Caffe" record).
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.2.cmp(&a.2))
    });

    let &(best, best_score, best_matched) = scored.first()?;
    if best_score < MIN_SCORE || best_matched == 0 {
        return None;
    }
    let best_title = normalized(&candidates[best].title);
    // Providers list the same work twice (preprint + published); a duplicate
    // title is not a rival.
    let rival = scored
        .iter()
        .skip(1)
        .find(|(i, ..)| normalized(&candidates[*i].title) != best_title);
    match rival {
        Some(&(_, score, matched))
            if best_score - score < MIN_MARGIN && matched * 10 > best_matched * 7 =>
        {
            None
        }
        _ => Some(best),
    }
}

/// (share of title words found, how many were found).
///
/// Words must appear in order, except that one word may be out of place: PDF
/// extraction occasionally moves a single word to a neighbouring spot (a word
/// hyphenated across lines lands at the start of its line). Two titles made of
/// the same words in different orders still score apart.
fn title_score(title: &[String], entry: &[String]) -> (f64, usize) {
    if title.is_empty() {
        return (0.0, 0);
    }
    let in_order = lcs(title, entry);
    let present = title.iter().filter(|w| entry.contains(w)).count();
    let found = in_order + (present - in_order).min(1);
    (found as f64 / title.len() as f64, found)
}

/// Longest common subsequence length, title words against entry words.
fn lcs(a: &[String], b: &[String]) -> usize {
    let mut prev = vec![0usize; b.len() + 1];
    let mut cur = vec![0usize; b.len() + 1];
    for x in a {
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = if x == y {
                prev[j] + 1
            } else {
                cur[j].max(prev[j + 1])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Lower-cased alphanumeric words. With `join_hyphens`, `deeply-supervised`
/// stays one word (`deeplysupervised`), matching PDF text where the hyphen
/// was lost; without, it splits in two.
fn words(s: &str, join_hyphens: bool) -> Vec<String> {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if matches!(c, '-' | '\u{2010}' | '\u{2011}' | '\u{ad}') && join_hyphens {
            continue;
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().map(str::to_string).collect()
}

fn significant(words: &[String]) -> Vec<String> {
    words
        .iter()
        .filter(|w| w.chars().count() > 2 && !STOPWORDS.contains(&w.as_str()))
        .cloned()
        .collect()
}

fn normalized(title: &str) -> String {
    words(title, true).join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn papers(titles: &[&str]) -> Vec<Paper> {
        titles
            .iter()
            .map(|t| Paper::new((*t).to_string()))
            .collect()
    }

    // Entries below are verbatim extractions from the ResNet PDF (arXiv:1512.03385).

    #[test]
    fn matches_despite_garbled_parse() {
        let refs = papers(&[
            "Going deeper with convolutions",
            "Microsoft COCO: Common Objects in Context",
            "Rich Feature Hierarchies for Accurate Object Detection and Semantic Segmentation",
        ]);
        let entry = "[44] C. Szegedy, W. Liu, Y. Jia, P. Sermanet, S. Reed, D. Anguelov, D. convolutions. \
                     V. Vanhoucke, and A. Rabinovich. Going deeper with In CVPR, 2015.";
        assert_eq!(match_reference(entry, &refs), Some(0));
        let entry = "[8] R. hierarchies Girshick, J. Donahue, T. Darrell, and J. Malik. Rich feature for \
                     accurate object detection and semantic segmentation. In CVPR, 2014.";
        assert_eq!(match_reference(entry, &refs), Some(2));
    }

    #[test]
    fn word_order_separates_anagram_titles() {
        let refs = papers(&[
            "Pattern Recognition and Neural Networks",
            "Neural Networks for Pattern Recognition",
        ]);
        let bishop = "[2] C. M. Bishop. Neural networks for pattern recognition. Oxford university press, 1995.";
        let ripley = "[34] B. D. Ripley. Pattern recognition and neural networks. Cambridge university press, 1996.";
        assert_eq!(match_reference(bishop, &refs), Some(1));
        assert_eq!(match_reference(ripley, &refs), Some(0));
    }

    #[test]
    fn longer_duplicate_beats_bare_title() {
        let refs = papers(&[
            "Caffe",
            "Caffe: Convolutional Architecture for Fast Feature Embedding",
        ]);
        let entry = "[19] Y. Jia, E. Shelhamer, J. Donahue. Caffe: Convolutional architecture for fast \
                     feature embedding. arXiv:1408.5093, 2014.";
        assert_eq!(match_reference(entry, &refs), Some(1));
    }

    #[test]
    fn joined_hyphen_still_matches() {
        let refs = papers(&["Deeply-Supervised Nets", "Highway Networks"]);
        let entry = "[24] C.-Y. Deeplysupervised Lee, S. Xie, P. Gallagher, Z. Zhang, and Z. Tu. nets. \
                     arXiv:1409.5185, 2014.";
        assert_eq!(match_reference(entry, &refs), Some(0));
    }

    #[test]
    fn duplicate_records_are_not_rivals() {
        let refs = papers(&["Efficient BackProp", "Efficient BackProp"]);
        assert!(match_reference("[23] Y. LeCun et al. Efficient backprop. 1998.", &refs).is_some());
    }

    #[test]
    fn absent_reference_is_not_forced() {
        // Maxout is not in the list; "Highway Networks" shares only "networks".
        let refs = papers(&[
            "Highway Networks",
            "Centering Neural Network Gradient Factors",
        ]);
        let entry = "[10] I. J. Goodfellow, D. Warde-Farley, M. Mirza, A. Courville, and Y. Bengio. \
                     Maxout networks. arXiv:1302.4389, 2013.";
        assert_eq!(match_reference(entry, &refs), None);
        assert_eq!(
            match_reference(
                "[25] M. Lin, Q. Chen, and S. Yan. Network in network. arXiv:1312.4400, 2013.",
                &refs
            ),
            None
        );
    }
}
