//! 京阪式アクセント（関西弁）。NJDの単語から拍ごとの高低を決める。
//!
//! 単語の型は関西弁アクセント辞書（`keihan/kansai_accent_dict.csv`、出典と許諾は`keihan/NOTICE.txt`）から
//! 引き、辞書にない語は東京式のアクセント型から推定する。助詞・助動詞は前の語の高低に続ける。

use std::{collections::HashMap, sync::LazyLock};

use open_jtalk::NjdFeature;
use serde::{Deserialize, Serialize};

/// アクセントの方言。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccentDialect {
    /// 東京式（Open JTalkのアクセント型そのまま）。
    #[default]
    Standard,
    /// 京阪式（関西弁）。
    Keihan,
}

/// 1語分の高低。`levels`は拍ごとに`H`（高）か`L`（低）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordAccent {
    /// 表層形。
    pub text: String,
    /// 発音（カタカナ）。
    pub pron: String,
    /// アクセント句の先頭か。
    pub phrase_start: bool,
    /// 拍ごとの高低。拍を持たない記号は空。
    pub levels: String,
}

/// 単語ごとの高低。拍を持たない記号は高低が空で、句を区切らない。
pub(crate) fn word_accents(features: &[NjdFeature], dialect: AccentDialect) -> Vec<WordAccent> {
    // 拍を持つ語をアクセント句（`chain_flag`が1なら前の句に続く）にまとめる
    let mut phrases: Vec<Vec<usize>> = vec![];
    for (index, feature) in features.iter().enumerate() {
        if feature.mora_size <= 0 {
            continue;
        }
        match phrases.last_mut() {
            Some(phrase) if feature.chain_flag == 1 => phrase.push(index),
            _ => phrases.push(vec![index]),
        }
    }
    let mut levels = vec![String::new(); features.len()];
    let mut starts = vec![false; features.len()];
    for phrase in &phrases {
        let words = phrase.iter().map(|&i| &features[i]).collect::<Vec<_>>();
        let phrase_levels = match dialect {
            AccentDialect::Standard => standard_phrase(&words),
            AccentDialect::Keihan => keihan_phrase(&words),
        };
        for (&index, word_levels) in phrase.iter().zip(phrase_levels) {
            levels[index] = word_levels;
        }
        starts[phrase[0]] = true;
    }
    features
        .iter()
        .zip(levels)
        .zip(starts)
        .map(|((feature, levels), phrase_start)| WordAccent {
            text: feature.string.clone(),
            pron: feature.pron.clone(),
            phrase_start,
            levels,
        })
        .collect()
}

const H: char = 'H';
const L: char = 'L';

/// 東京式: 句頭の1拍目は低く（頭高なら高く）、アクセント核の拍まで高い。
fn standard_phrase(words: &[&NjdFeature]) -> Vec<String> {
    let accent = words[0].acc.max(0) as usize;
    let mut position = 0;
    words
        .iter()
        .map(|word| {
            (0..word.mora_size as usize)
                .map(|_| {
                    position += 1;
                    let high = match accent {
                        0 => position > 1,
                        1 => position == 1,
                        _ => position > 1 && position <= accent,
                    };
                    if high { H } else { L }
                })
                .collect()
        })
        .collect()
}

/// 京阪式: 句頭の語の型を決め、続く語は前の高低に続ける。
fn keihan_phrase(words: &[&NjdFeature]) -> Vec<String> {
    let accent = words[0].acc.max(0) as usize;
    let mut state = State::default();
    let mut levels: Vec<char> = vec![];
    let mut result = vec![];
    for (index, word) in words.iter().enumerate() {
        let moras = word.mora_size as usize;
        let word_levels = if index == 0 {
            head_levels(word, accent, &mut state)
        } else {
            following_levels(word, moras, levels.len(), accent, &levels, &mut state)
        };
        levels.extend(&word_levels);
        result.push(word_levels.into_iter().collect());
    }
    result
}

#[derive(Default)]
struct State {
    /// 低起の語がまだ上がっていない（次の拍で上がる）。
    pending_rise: bool,
    /// 語末で下がる語（辞書の`↘`）のあと。
    falls_after: bool,
    /// 句頭の語が辞書にない（東京式の核の位置を使う）。
    follows_tokyo: bool,
}

fn head_levels(word: &NjdFeature, accent: usize, state: &mut State) -> Vec<char> {
    let moras = word.mora_size as usize;
    if word.pos == "名詞" && word.pos_group1 == "数" {
        // 数は高く始め、東京式の核で下がる
        state.follows_tokyo = true;
        return (1..=moras)
            .map(|i| if accent == 0 || i <= accent { H } else { L })
            .collect();
    }
    if let Some(entry) = DICTIONARY.lookup(word) {
        let conjugated = matches!(&*word.pos, "動詞" | "形容詞") && word.string != word.orig;
        if conjugated {
            // 活用形は語幹に起式だけを当てる: 高起は高く、低起は低く次の拍で上がる
            if entry.levels.first() == Some(&H) {
                return vec![H; moras];
            }
            state.pending_rise = true;
            return vec![L; moras];
        }
        if entry.levels.len() == moras {
            state.falls_after = entry.falls;
            state.pending_rise = entry.levels.iter().all(|&level| level == L);
            return entry.levels.clone();
        }
    }
    // 辞書にない語は東京式から: 平板は高く平ら、頭高は低く始めて語末で上がる、中高・尾高は高く始めて同じ核で下がる
    state.follows_tokyo = true;
    match accent {
        0 => vec![H; moras],
        1 if moras == 1 => {
            state.pending_rise = true;
            vec![L]
        }
        1 => (1..=moras)
            .map(|i| if i == moras { H } else { L })
            .collect(),
        _ => (1..=moras)
            .map(|i| if i <= accent { H } else { L })
            .collect(),
    }
}

fn following_levels(
    word: &NjdFeature,
    moras: usize,
    offset: usize,
    accent: usize,
    before: &[char],
    state: &mut State,
) -> Vec<char> {
    if state.pending_rise {
        // 低起の語のあとは次の拍で上がり、すぐ下がる
        state.pending_rise = false;
        return (0..moras).map(|i| if i == 0 { H } else { L }).collect();
    }
    let fallen = state.falls_after || has_fallen(before);
    if fallen || is_sentence_final(word) || is_negative(word) {
        return vec![L; moras];
    }
    if !state.follows_tokyo {
        return vec![H; moras];
    }
    (1..=moras)
        .map(|i| {
            if accent > 0 && offset + i > accent {
                L
            } else {
                H
            }
        })
        .collect()
}

/// 高い拍のあとに低い拍がある。
fn has_fallen(levels: &[char]) -> bool {
    levels
        .iter()
        .skip_while(|&&level| level != H)
        .any(|&level| level == L)
}

/// 文末の終助詞と、断定の「や」。
fn is_sentence_final(word: &NjdFeature) -> bool {
    (word.pos == "助詞" && word.pos_group1 == "終助詞")
        || (word.pos == "助動詞" && word.string == "や")
}

/// 打ち消しの助動詞（へん・ひん・ん・ない・ぬ）。
fn is_negative(word: &NjdFeature) -> bool {
    word.pos == "助動詞" && matches!(&*word.orig, "へん" | "ひん" | "ん" | "ない" | "ぬ")
}

struct Entry {
    levels: Vec<char>,
    /// 語末で下がる（`↘`）。
    falls: bool,
    /// 表記（空なら読みだけの見出し）。
    has_original: bool,
}

struct Dictionary {
    entries: Vec<Entry>,
    by_original: HashMap<String, Vec<usize>>,
    by_reading: HashMap<String, Vec<usize>>,
}

static DICTIONARY: LazyLock<Dictionary> =
    LazyLock::new(|| Dictionary::parse(include_str!("keihan/kansai_accent_dict.csv")));

impl Dictionary {
    fn parse(csv: &str) -> Self {
        let mut dictionary = Self {
            entries: vec![],
            by_original: HashMap::new(),
            by_reading: HashMap::new(),
        };
        for row in parse_csv(csv).into_iter().skip(1) {
            let [word, original, _pos, accent, ..] = &row[..] else {
                continue;
            };
            // 複数の語形（「・」区切り）は最初の形だけ使う
            let accent = accent.split('・').next().unwrap_or_default();
            let levels = accent
                .chars()
                .filter(|c| *c == H || *c == L)
                .collect::<Vec<_>>();
            if levels.is_empty() {
                continue;
            }
            let index = dictionary.entries.len();
            dictionary.entries.push(Entry {
                levels,
                falls: accent.contains('↘'),
                has_original: !original.is_empty(),
            });
            if let Some(reading) = word.split('・').next() {
                dictionary
                    .by_reading
                    .entry(reading.to_owned())
                    .or_default()
                    .push(index);
            }
            for original in original.split('・').filter(|s| !s.is_empty()) {
                dictionary
                    .by_original
                    .entry(original.to_owned())
                    .or_default()
                    .push(index);
            }
        }
        dictionary
    }

    /// 内容語の型。表記（原形）で引き、なければ表記のない見出しを読みで引く。
    fn lookup(&self, word: &NjdFeature) -> Option<&Entry> {
        if !matches!(
            &*word.pos,
            "名詞" | "動詞" | "形容詞" | "副詞" | "連体詞" | "感動詞"
        ) {
            return None;
        }
        let by_original = [&word.orig, &word.string]
            .into_iter()
            .find_map(|key| self.by_original.get(key.as_str()))
            .and_then(|ids| ids.first());
        let by_reading = || {
            self.by_reading
                .get(&hiragana(&word.read))?
                .iter()
                .find(|&&id| !self.entries[id].has_original)
        };
        by_original.or_else(by_reading).map(|&id| &self.entries[id])
    }
}

fn hiragana(katakana: &str) -> String {
    katakana
        .chars()
        .map(|c| match c {
            'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// 引用符（改行を含む欄）に対応したCSVの読み込み。
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            ('\r', false) => {}
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use open_jtalk::NjdFeature;
    use pretty_assertions::assert_eq;

    use super::{AccentDialect, word_accents};

    fn word(
        string: &str,
        pos: &str,
        pos1: &str,
        orig: &str,
        read: &str,
        acc: i32,
        mora: i32,
        chain: i32,
    ) -> NjdFeature {
        NjdFeature {
            string: string.to_owned(),
            pos: pos.to_owned(),
            pos_group1: pos1.to_owned(),
            pos_group2: "*".to_owned(),
            pos_group3: "*".to_owned(),
            ctype: "*".to_owned(),
            cform: "*".to_owned(),
            orig: orig.to_owned(),
            read: read.to_owned(),
            pron: read.to_owned(),
            acc,
            mora_size: mora,
            chain_rule: "*".to_owned(),
            chain_flag: chain,
        }
    }

    fn levels(features: &[NjdFeature], dialect: AccentDialect) -> Vec<String> {
        word_accents(features, dialect)
            .into_iter()
            .map(|w| format!("{}:{}", w.text, w.levels))
            .collect()
    }

    /// 東京式はOpen JTalkの型そのまま: 「傘は」頭高、「時間や」平板。
    #[test]
    fn standard_follows_open_jtalk() {
        let features = [
            word("傘", "名詞", "一般", "傘", "カサ", 1, 2, -1),
            word("は", "助詞", "係助詞", "は", "ワ", 0, 1, 1),
            word("時間", "名詞", "一般", "時間", "ジカン", 0, 3, 0),
            word("や", "助動詞", "*", "や", "ヤ", 1, 1, 1),
        ];
        assert_eq!(
            levels(&features, AccentDialect::Standard),
            ["傘:HL", "は:L", "時間:LHH", "や:H"]
        );
    }

    /// 辞書の型: 傘 LH（低起、助詞で高いまま）、時間 HLL（下がったあとは低い）。
    #[test]
    fn keihan_uses_the_dictionary() {
        let features = [
            word("傘", "名詞", "一般", "傘", "カサ", 1, 2, -1),
            word("は", "助詞", "係助詞", "は", "ワ", 0, 1, 1),
            word("時間", "名詞", "一般", "時間", "ジカン", 0, 3, 0),
            word("や", "助動詞", "*", "や", "ヤ", 1, 1, 1),
            word("で", "助詞", "終助詞", "で", "デ", 1, 1, 1),
        ];
        assert_eq!(
            levels(&features, AccentDialect::Keihan),
            ["傘:LH", "は:H", "時間:HLL", "や:L", "で:L"]
        );
    }

    /// 高起の動詞（要る HH）は語幹が高く、打ち消しの「へん」で下がる。
    #[test]
    fn a_high_verb_falls_at_hen() {
        let features = [
            word("いら", "動詞", "自立", "いる", "イラ", 0, 2, -1),
            word("へん", "助動詞", "*", "へん", "ヘン", 1, 2, 1),
            word("で", "助詞", "終助詞", "で", "デ", 1, 1, 1),
        ];
        assert_eq!(
            levels(&features, AccentDialect::Keihan),
            ["いら:HH", "へん:LL", "で:L"]
        );
    }

    /// 低起の動詞（出る LH）は語幹が低く、次の拍で上がってから下がる。
    #[test]
    fn a_low_verb_rises_on_the_next_mora() {
        let features = [
            word("出", "動詞", "自立", "出る", "デ", 2, 1, -1),
            word("とる", "動詞", "非自立", "とる", "トル", 1, 2, 1),
            word("で", "助詞", "終助詞", "で", "デ", 1, 1, 1),
        ];
        assert_eq!(
            levels(&features, AccentDialect::Keihan),
            ["出:L", "とる:HL", "で:L"]
        );
    }

    /// 辞書にない語は東京式から: 平板は高く平ら、中高は高く始めて同じ位置で下がる。
    #[test]
    fn unknown_words_follow_the_tokyo_type() {
        let features = [
            word("朝会", "名詞", "一般", "朝会", "チョウカイ", 0, 4, -1),
            word("の", "助詞", "連体化", "の", "ノ", 1, 1, 1),
            word(
                "折りたたみ傘",
                "名詞",
                "一般",
                "折りたたみ傘",
                "オリタタミガサ",
                6,
                7,
                0,
            ),
            word("が", "助詞", "格助詞", "が", "ガ", 0, 1, 1),
        ];
        assert_eq!(
            levels(&features, AccentDialect::Keihan),
            ["朝会:HHHH", "の:H", "折りたたみ傘:HHHHHHL", "が:L"]
        );
    }

    /// 数は高く始める（「10分」ジュッ'プン → HLLL）。
    #[test]
    fn numbers_start_high() {
        let features = [
            word("十", "名詞", "数", "十", "ジュウ", 1, 2, -1),
            word("分", "名詞", "接尾", "分", "フン", 1, 2, 1),
        ];
        assert_eq!(levels(&features, AccentDialect::Keihan), ["十:HL", "分:LL"]);
    }
}
