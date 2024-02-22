// https://murilo.wordpress.com/2011/02/01/fast-and-easy-levenshtein-distance-using-a-trie-in-c/

use std::{
    cmp,
    collections::{BTreeMap, HashSet},
    hash::Hash,
};

use derivative::Derivative;

#[derive(Derivative)]
#[derivative(Debug, Default(bound = ""))]
pub struct Trie<T> {
    next: BTreeMap<char, Trie<T>>,
    word_len: Option<usize>,
    data: Vec<T>,
}

impl<T: Hash + Eq> Trie<T> {
    pub fn insert(&mut self, word: &[char], data: T) {
        let mut word = word.to_owned();
        word.insert(0, '$');
        let word = &word;

        let mut n = self;
        for ch in word {
            n = n.next.entry(*ch).or_default()
        }
        if n.word_len.is_none() {
            n.word_len = Some(word.len());
        }
        n.data.push(data);
    }

    pub fn clear(&mut self) {
        self.next.clear();
    }

    pub fn search<'a>(&'a self, max_distance: i32, word: &[char]) -> HashSet<ResultEntry<'a, T>> {
        let mut word = word.to_owned();
        word.insert(0, '$');
        let word = &word;
        let sz = word.len();
        let current_row: Vec<i32> = (0..=sz).map(|x| x as i32).collect();

        let mut results = HashSet::new();
        for ch in word {
            if let Some(trie) = self.next.get(ch) {
                trie.search_recursive(max_distance, *ch, &current_row, word, &mut results);
            }
        }

        results
    }

    fn search_recursive<'a>(
        &'a self,
        max_distance: i32,
        ch: char,
        last_row: &[i32],
        word: &[char],
        results: &mut HashSet<ResultEntry<'a, T>>,
    ) {
        let sz = last_row.len();
        let mut current_row: Vec<i32> = vec![0; sz];
        current_row[0] = last_row[0] + 1;

        for i in 1..sz {
            let insert_or_del = cmp::min(current_row[i - 1] + 1, last_row[i] + 1);
            let replace = if word[i - 1] == ch {
                last_row[i - 1]
            } else {
                last_row[i - 1] + 1
            };

            current_row[i] = cmp::min(insert_or_del, replace);
        }

        let dist = current_row[sz - 1];
        if dist <= max_distance
            // 排除编辑距离等于源字符串或匹配字符串之一的长度的情况
            && self
                .word_len
                .is_some_and(|x| (dist as usize) < cmp::max(word.len(), x) - 1) // '$' 前缀
        {
            for data in &self.data {
                let entry = ResultEntry(dist, data);

                match results.get(&entry) {
                    Some(existing) if existing.0 <= dist => {}
                    _ => {
                        results.replace(entry);
                    }
                }
            }
        }

        if current_row.iter().any(|x| *x <= max_distance) {
            for (ch, trie) in self.next.iter() {
                trie.search_recursive(max_distance, *ch, &current_row, word, results);
            }
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct ResultEntry<'a, T>(pub i32, pub &'a T);

impl<'a, T: PartialEq> PartialEq for ResultEntry<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.1 == other.1
    }
}

impl<'a, T: Eq> Eq for ResultEntry<'a, T> {}

impl<'a, T: Hash> Hash for ResultEntry<'a, T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.1.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trie_test() {
        let mut trie = Trie::default();

        trie.insert(&['h', 'e', 'l', 'l', 'o'], "hello");
        trie.insert(&['h', 'e', 'l', 'l', 'y'], "helly");
        trie.insert(&['h', 'e', 'l', 'l', 'y'], "helly2");

        assert_eq!(
            trie.search(1, &['h', 'e', 'l', 'l', 'o']),
            HashSet::from([
                ResultEntry(0, &"hello"),
                ResultEntry(1, &"helly"),
                ResultEntry(1, &"helly2")
            ])
        );
        assert_eq!(
            trie.search(1, &['h', 'e', 'l', 'l', 'y']),
            HashSet::from([
                ResultEntry(0, &"helly"),
                ResultEntry(0, &"helly2"),
                ResultEntry(1, &"hello")
            ])
        );
        assert_eq!(
            trie.search(2, &['h', 'e', 'l', 'o']),
            HashSet::from([
                ResultEntry(1, &"hello"),
                ResultEntry(2, &"helly"),
                ResultEntry(2, &"helly2")
            ])
        );
        assert_eq!(
            trie.search(2, &['m', 'e', 'l', 'l', 'o']),
            HashSet::from([
                ResultEntry(1, &"hello"),
                ResultEntry(2, &"helly"),
                ResultEntry(2, &"helly2")
            ])
        );
        assert_eq!(
            trie.search(1, &['m', 'e', 'l', 'l', 'o']),
            HashSet::from([ResultEntry(1, &"hello")])
        );
        assert_eq!(
            trie.search(0, &['m', 'e', 'l', 'l', 'o']),
            HashSet::from([])
        );
    }
}
