use std::{collections::HashMap, fmt::Debug};

use rust_stemmers::{Algorithm, Stemmer};

const STOPWORDS: &[&str] = &[
    "a", "and", "be", "have", "i", "in", "of", "that", "the", "to",
];

pub fn tokenize<S: Into<String>>(text: S) -> Vec<String> {
    let en_stemmer = Stemmer::create(Algorithm::English);

    text.into()
        .split(|c: char| !c.is_alphabetic() && !c.is_numeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        // .filter(|token| !STOPWORDS.contains(&token.as_str())) // delete common words
        .map(|token| en_stemmer.stem(token.as_str()).to_string())
        .collect()
}

pub trait TokenProvider {
    fn get_tokens(&self) -> Vec<String>;
}

// Content requried for kind-of full-text search functionalities.
#[derive(Debug, Default)]
pub struct FTS<T> {
    /// Whole avialable content for search.
    content: Vec<T>,
    /// Contains <token - content's index> mapping.
    indexes: HashMap<String, Vec<usize>>,
}

impl<T> FTS<T>
where
    T: Clone + Ord + Debug,
{
    pub fn push(&mut self, content: T)
    where
        T: TokenProvider,
    {
        self.content.push(content.clone());

        let idx = self.content.len() - 1;
        content
            .get_tokens()
            .into_iter()
            .for_each(|token| self.indexes.entry(token).or_default().push(idx));
    }

    /// Takes statement, tokenizes it and retruns all possible content for those tokens.
    pub fn search<S: Into<String>>(&self, statement: S) -> Option<Vec<T>> {
        let statement: String = statement.into();

        let tokens = tokenize(statement);

        let mut scores: HashMap<usize, usize> = HashMap::default();
        // calculate scores for each contents
        tokens.into_iter().for_each(|token| {
            // TODO: if token matches only by starts_with it should get less points

            // self.indexes.get(&token).and_then(|indexes| {
            //     indexes.iter().for_each(|i| {
            //         scores.entry(*i).and_modify(|v| *v += 1).or_default();
            //     });
            //     None::<Vec<usize>>
            // });
            self.indexes.iter().for_each(|(tkn, inxs)| {
                if tkn.starts_with(token.as_str()) {
                    inxs.into_iter().for_each(|inx| {
                        scores.entry(*inx).and_modify(|v| *v += 1).or_default();
                    })
                }
            });
        });

        // change into vector and sort
        let mut hash_vec: Vec<(usize, (usize, T))> = scores
            .into_iter()
            .map(|(inx, value)| (inx, (value, self.content.get(inx).unwrap().clone())))
            .collect();

        hash_vec.sort_by(|a, b| {
            let cmp = b.1 .0.cmp(&a.1 .0); // DESC by score
            if cmp.is_eq() {
                a.1 .1.cmp(&b.1 .1) // ASC by alpabetically
            } else {
                cmp
            }
        });
        let mut results: Vec<T> = vec![];

        for inx in hash_vec.into_iter().map(|a| a.0) {
            if let Some(result) = self.content.get(inx) {
                results.push(result.clone());
            }
        }

        Some(results)
    }
}

#[cfg(test)]
mod tests {
    use super::{tokenize, TokenProvider, FTS};

    impl TokenProvider for &str {
        fn get_tokens(&self) -> Vec<String> {
            tokenize(self.to_string())
        }
    }

    #[test]
    fn test_ordering() {
        let mut a = vec![
            (0, "He strives to keep the best lawn in the neighborhood."),
            (
                0,
                "I was very proud of my nickname throughout high school but today- I couldn’t be any different to what my nickname was.",
            ),
        ];
        a.sort_by(|a, b| b.cmp(a));
        println!("{:?}", a)
    }

    #[test]
    fn test_fts() -> anyhow::Result<()> {
        let mut fts: FTS<&str> = FTS::default();
        fts.push("Today is the day I'll finally know what brick tastes like.");
        fts.push("I am counting my calories, yet I really want dessert.");
        fts.push("I was very proud of my nickname throughout high school but today- I couldn’t be any different to what my nickname was.");
        fts.push("Honestly, I didn't care much for the first season, so I didn't bother with the second.");
        fts.push("his seven-layer cake only had six layers.");
        fts.push("He was so preoccupied with whether or not he could that he failed to stop to consider if he should.");
        fts.push("He didn't heed the warning and it had turned out surprisingly well.");
        fts.push("He strives to keep the best lawn in the neighborhood.");
        fts.push("The most exciting eureka moment I've had was when I realized that the instructions on food packets were just guidelines.");
        fts.push("The sun had set and so had his dreams.");

        assert_eq!(fts.search("so").unwrap().sort(), vec![
            "He was so preoccupied with whether or not he could that he failed to stop to consider if he should.",
            "Honestly, I didn't care much for the first season, so I didn't bother with the second.",
            "The sun had set and so had his dreams.",
        ].sort());

        assert_eq!(
            fts.search("had").unwrap()[0],
            "The sun had set and so had his dreams."
        );
        assert_eq!(
            fts.search("to").unwrap(), vec![
                "He was so preoccupied with whether or not he could that he failed to stop to consider if he should.", 
                "I was very proud of my nickname throughout high school but today- I couldn’t be any different to what my nickname was.",
                "He strives to keep the best lawn in the neighborhood.",
                "Today is the day I'll finally know what brick tastes like."
            ]
        );
        Ok(())
    }
}
