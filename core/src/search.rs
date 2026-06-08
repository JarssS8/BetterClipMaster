//! Fuzzy ranking of clipboard items, Alfred-style.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::model::ClipItem;

/// What text we match a query against for a given item.
fn haystack(item: &ClipItem) -> String {
    if item.content.is_empty() {
        item.preview.clone()
    } else {
        format!("{} {}", item.preview, item.content)
    }
}

/// Rank `items` against `query`.
///
/// An empty query returns all items in their original order. A non-empty query
/// returns only matching items, best match first. Ties keep input order.
pub fn rank(items: &[ClipItem], query: &str) -> Vec<ClipItem> {
    if query.trim().is_empty() {
        return items.to_vec();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

    let mut scored: Vec<(u32, &ClipItem)> = items
        .iter()
        .filter_map(|item| {
            let hay = haystack(item);
            let mut buf = Vec::new();
            let utf = Utf32Str::new(&hay, &mut buf);
            pattern.score(utf, &mut matcher).map(|score| (score, item))
        })
        .collect();

    // Stable sort by descending score keeps input order for ties.
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, item)| item.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ClipKind;

    fn item(id: i64, content: &str) -> ClipItem {
        ClipItem {
            id,
            kind: ClipKind::Text,
            content: content.to_string(),
            blob: None,
            preview: content.to_string(),
            pinned: false,
            created_at: id,
            hash: format!("h{id}"),
        }
    }

    #[test]
    fn empty_query_returns_all_in_order() {
        let items = vec![item(1, "alpha"), item(2, "beta")];
        let ranked = rank(&items, "");
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].id, 1);
        assert_eq!(ranked[1].id, 2);
    }

    #[test]
    fn ranks_better_match_first() {
        let items = vec![
            item(1, "readme.md"),
            item(2, "config.env"),
            item(3, "main.rs"),
        ];
        let ranked = rank(&items, "cfg");
        assert!(!ranked.is_empty());
        assert_eq!(ranked[0].content, "config.env");
    }

    #[test]
    fn no_match_returns_empty() {
        let items = vec![item(1, "alpha"), item(2, "beta")];
        let ranked = rank(&items, "zzzzz");
        assert!(ranked.is_empty());
    }

    #[test]
    fn matches_are_case_insensitive() {
        let items = vec![item(1, "HelloWorld")];
        let ranked = rank(&items, "hello");
        assert_eq!(ranked.len(), 1);
    }
}
