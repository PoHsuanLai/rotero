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
/// Titles with at most this many significant words must appear as a whole
/// clause of the entry, not merely as words somewhere in it.
const SHORT_TITLE: usize = 3;

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
///
/// A short title ("Dropout", "Deep Learning", "Highway Networks") is far too
/// easy to find as loose words — "Deep *residual* learning…", a venue called
/// *Neural Networks* — and the reference list is often incomplete, so the real
/// target may be missing and nothing would out-rank it. Those must instead
/// make up a whole clause of the entry ("… Schmidhuber. Highway networks.
/// arXiv …").
pub fn match_reference(entry: &str, candidates: &[Paper]) -> Option<usize> {
    let entry_forms = [words(entry, false), words(entry, true)];
    let entry_clauses = clauses(entry);

    let mut scored: Vec<(usize, f64, usize)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let sig = significant(&words(&p.title, false)).len();
            if sig == 0 {
                return None;
            }
            if sig <= SHORT_TITLE {
                let whole = [false, true].into_iter().any(|join| {
                    let title = words(&p.title, join);
                    entry_clauses.iter().any(|c| c[usize::from(join)] == title)
                });
                return whole.then_some((i, 1.0, sig));
            }
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

/// The entry split into sentences and quoted spans, each as (split, joined)
/// words. Not split at commas: a title ends in a full stop (or sits in quotes),
/// whereas a venue is followed by ", volume:pages", so "Neural Networks, 61:85"
/// never reads as a clause equal to a title "Neural Networks".
///
/// A clause with a subtitle also yields its head ("The Open Images Dataset V4:
/// Unified image classification…" → "The Open Images Dataset V4"), since
/// providers often store a title without its subtitle.
fn clauses(entry: &str) -> Vec<[Vec<String>; 2]> {
    entry
        .split(['.', '"', '\u{201c}', '\u{201d}', '?', '!'])
        .flat_map(|c| {
            let head = c.split_once(':').map(|(h, _)| h);
            std::iter::once(c).chain(head)
        })
        .map(|c| [words(c, false), words(c, true)])
        .filter(|[w, _]| !w.is_empty())
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
        // As extracted from ResNet, title hyphenation now fixed but the
        // parse still takes the author tail as the title.
        let entry = "[44] C. Szegedy, W. Liu, Y. Jia, P. Sermanet, S. Reed, D. Anguelov, D. \
                     V. Vanhoucke, and A. Rabinovich. Going deeper with convolutions. In CVPR, 2015.";
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
        let entry = "[24] C.-Y. Lee, S. Xie, P. Gallagher, Z. Zhang, and Z. Tu. Deeplysupervised nets. \
                     arXiv:1409.5185, 2014.";
        assert_eq!(match_reference(entry, &refs), Some(0));
    }

    #[test]
    fn duplicate_records_are_not_rivals() {
        let refs = papers(&["Efficient BackProp", "Efficient BackProp"]);
        assert!(match_reference("[23] Y. LeCun et al. Efficient backprop. 1998.", &refs).is_some());
    }

    #[test]
    fn short_title_needs_a_whole_clause() {
        let refs = papers(&["Deep Learning", "Neural Networks", "Image Recognition"]);
        for entry in [
            "[13] K. He, X. Zhang, S. Ren, and J. Sun. Deep residual learning for image recognition. In CVPR, 2016.",
            "[5] A. Graves. Generating sequences. Neural Networks, 61:85-117, 2015.",
        ] {
            assert_eq!(match_reference(entry, &refs), None, "{entry}");
        }
        let entry =
            "[30] Y. LeCun, Y. Bengio, and G. Hinton. Deep learning. Nature, 521:436-444, 2015.";
        assert_eq!(match_reference(entry, &refs), Some(0));
    }

    #[test]
    fn short_title_matches_head_before_subtitle() {
        let refs = papers(&["The Open Images Dataset V4", "Highway Networks"]);
        let entry = "[49] A. Kuznetsova, H. Rom, N. Alldrin. The open images dataset V4: Unified image \
                     classification, object detection, and visual relationship detection at scale. IJCV, 2020.";
        assert_eq!(match_reference(entry, &refs), Some(0));
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
