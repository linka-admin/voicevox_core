//! 英文（スペースで区切られた2語以上の英単語の並び）の、カタカナ英語としての読み。
//!
//! 英文をMeCabで解析すると、短い語が1文字ずつ読まれたり（「it」→「アイ」「ティー」）、辞書の英単語の
//! アクセントが崩れていたりするため、英文はMeCabに通す前に切り出し、単語ごとに読みを決める。
//!
//! - 機能語（冠詞・前置詞・代名詞・助動詞など）は、表の読みを使い、後ろの内容語と1つのアクセント句に
//!   する（例: 「to the park」→「トゥザパ'ーク」）。後ろに内容語がなければ前のアクセント句に付ける。
//! - 内容語は、辞書で1語として読めればその読みを、読めなければkanalizerによる読みを使い、
//!   [`katakana_features`]で解析し直してアクセントを付ける。

use open_jtalk::NjdFeature;

use super::katakana_english::{
    alphabets_to_kana, katakana_features, loanword_accent, split_moras, to_hankaku_alphabet,
};

/// 機能語の読み。
const FUNCTION_WORDS: &[(&str, &str)] = &[
    ("a", "ア"),
    ("about", "アバウト"),
    ("am", "アム"),
    ("an", "アン"),
    ("and", "アンド"),
    ("are", "アー"),
    ("as", "アズ"),
    ("at", "アット"),
    ("be", "ビー"),
    ("been", "ビーン"),
    ("but", "バット"),
    ("by", "バイ"),
    ("can", "キャン"),
    ("can't", "キャント"),
    ("could", "クッド"),
    ("did", "ディド"),
    ("didn't", "ディドント"),
    ("do", "ドゥ"),
    ("does", "ダズ"),
    ("doesn't", "ダズント"),
    ("don't", "ドント"),
    ("for", "フォー"),
    ("from", "フロム"),
    ("had", "ハッド"),
    ("has", "ハズ"),
    ("have", "ハブ"),
    ("he", "ヒー"),
    ("he's", "ヒーズ"),
    ("her", "ハー"),
    ("him", "ヒム"),
    ("his", "ヒズ"),
    ("i", "アイ"),
    ("i'd", "アイド"),
    ("i'll", "アイル"),
    ("i'm", "アイム"),
    ("i've", "アイブ"),
    ("if", "イフ"),
    ("in", "イン"),
    ("is", "イズ"),
    ("isn't", "イズント"),
    ("it", "イット"),
    ("it's", "イッツ"),
    ("its", "イッツ"),
    ("let's", "レッツ"),
    ("me", "ミー"),
    ("my", "マイ"),
    ("not", "ノット"),
    ("of", "オブ"),
    ("on", "オン"),
    ("or", "オア"),
    ("our", "アワー"),
    ("she", "シー"),
    ("she's", "シーズ"),
    ("should", "シュッド"),
    ("so", "ソー"),
    ("than", "ザン"),
    ("that", "ザット"),
    ("that's", "ザッツ"),
    ("the", "ザ"),
    ("their", "ゼア"),
    ("them", "ゼム"),
    ("then", "ゼン"),
    ("there", "ゼア"),
    ("these", "ディーズ"),
    ("they", "ゼイ"),
    ("they're", "ゼア"),
    ("this", "ディス"),
    ("those", "ゾーズ"),
    ("to", "トゥ"),
    ("us", "アス"),
    ("was", "ワズ"),
    ("we", "ウィー"),
    ("we're", "ウィア"),
    ("were", "ワー"),
    ("will", "ウィル"),
    ("with", "ウィズ"),
    ("won't", "ウォント"),
    ("would", "ウッド"),
    ("you", "ユー"),
    ("you'll", "ユール"),
    ("you're", "ユア"),
    ("your", "ユア"),
];

/// 辞書とkanalizerのどちらでも不自然になる内容語の読み。
///
/// 疑問詞は英語でも強く読むため、機能語でなく内容語として扱う。
const CONTENT_WORDS: &[(&str, &str)] = &[
    ("hello", "ハロー"),
    ("hi", "ハイ"),
    ("how", "ハウ"),
    ("no", "ノー"),
    ("okay", "オーケー"),
    ("sorry", "ソーリー"),
    ("today", "トゥデイ"),
    ("tomorrow", "トゥモロー"),
    ("what", "ワット"),
    ("what's", "ワッツ"),
    ("when", "ウェン"),
    ("where", "ウェア"),
    ("who", "フー"),
    ("why", "ワイ"),
    ("yes", "イエス"),
];

/// 2語で決まった読みをする句。
const PHRASES: &[(&str, &str, &str)] = &[("thank", "you", "サンキュー")];

/// テキストを、英文とそれ以外に分けたもの。
#[derive(PartialEq, Debug)]
pub(super) enum Segment<'a> {
    Text(&'a str),
    /// 半角にした英単語の並び。アポストロフィは`'`にする。
    English(Vec<String>),
}

/// テキストから、スペースで区切られた2語以上の英単語の並びを切り出す。
pub(super) fn split_english_runs(text: &str) -> Vec<Segment<'_>> {
    let tokens = tokenize(text);
    let mut segments = vec![];
    let mut text_start = 0;
    let mut i = 0;
    while i < tokens.len() {
        // 単語、(スペース、単語)の繰り返し、の最長の並び
        let mut words = vec![];
        let mut j = i;
        while let Some(&Token::Word(start, end)) = tokens.get(j) {
            words.push((start, end));
            match (tokens.get(j + 1), tokens.get(j + 2)) {
                (Some(Token::Space), Some(Token::Word(..))) => j += 2,
                _ => break,
            }
        }
        if words.len() < 2 {
            i += 1;
            continue;
        }
        let (run_start, run_end) = (words[0].0, words[words.len() - 1].1);
        if text_start < run_start {
            segments.push(Segment::Text(&text[text_start..run_start]));
        }
        segments.push(Segment::English(
            words
                .iter()
                .map(|&(start, end)| normalize_word(&text[start..end]))
                .collect(),
        ));
        text_start = run_end;
        i = j + 1;
    }
    if text_start < text.len() {
        segments.push(Segment::Text(&text[text_start..]));
    }
    segments
}

#[derive(Clone, Copy, Debug)]
enum Token {
    /// バイト位置の範囲。
    Word(usize, usize),
    Space,
    Other,
}

fn tokenize(text: &str) -> Vec<Token> {
    let chars = text.char_indices().collect::<Vec<_>>();
    let is_letter = |k: usize| {
        chars
            .get(k)
            .is_some_and(|&(_, c)| to_hankaku_alphabet(c).is_some())
    };
    let mut tokens = vec![];
    let mut k = 0;
    while k < chars.len() {
        let (start, c) = chars[k];
        if is_letter(k) {
            let mut end = k + 1;
            // 単語の中のアポストロフィは単語に含める（例: 「I'm」）
            while is_letter(end)
                || (chars.get(end).is_some_and(|&(_, c)| is_apostrophe(c)) && is_letter(end + 1))
            {
                end += 1;
            }
            let end_byte = chars.get(end).map_or(text.len(), |&(i, _)| i);
            tokens.push(Token::Word(start, end_byte));
            k = end;
        } else {
            tokens.push(if matches!(c, ' ' | '　' | '\t') {
                Token::Space
            } else {
                Token::Other
            });
            k += 1;
        }
    }
    // 連続するスペースは1つにまとめる
    tokens.dedup_by(|a, b| matches!((a, b), (Token::Space, Token::Space)));
    tokens
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | '＇')
}

fn normalize_word(word: &str) -> String {
    word.chars()
        // アルファベット以外はアポストロフィ
        .map(|c| to_hankaku_alphabet(c).unwrap_or('\''))
        .collect()
}

/// 英文の単語の並びを、NJDの特徴量にする。
///
/// - `to_kana`: 小文字の英単語をカタカナに変換する。
/// - `reanalyze`: 文字列をNJDの特徴量に解析する。辞書の引き当てと、カタカナの解析し直しに使う。
pub(super) fn english_features(
    words: &[String],
    to_kana: &mut dyn FnMut(&str) -> anyhow::Result<String>,
    reanalyze: &mut dyn FnMut(&str) -> anyhow::Result<Vec<NjdFeature>>,
) -> anyhow::Result<Vec<NjdFeature>> {
    let mut features = vec![];
    // 後ろの内容語に付ける機能語
    let mut pending = vec![];
    let mut i = 0;
    while i < words.len() {
        let (word, lower) = (&words[i], words[i].to_ascii_lowercase());
        let phrase = words.get(i + 1).and_then(|next| {
            let next = next.to_ascii_lowercase();
            PHRASES
                .iter()
                .find(|&&(first, second, _)| first == lower && second == next)
        });
        if let Some(&(_, _, kana)) = phrase {
            let string = format!("{word} {}", words[i + 1]);
            let content = katakana_features(&string, kana, reanalyze)?;
            push_phrase(&mut features, std::mem::take(&mut pending), content);
            i += 2;
            continue;
        }
        if let Some(kana) = lookup(FUNCTION_WORDS, &lower) {
            pending.push(function_word(word, kana));
        } else {
            let kana = content_word_kana(word, &lower, to_kana, reanalyze)?;
            let content = katakana_features(word, &kana, reanalyze)?;
            push_phrase(&mut features, std::mem::take(&mut pending), content);
        }
        i += 1;
    }
    flush_trailing_function_words(&mut features, pending);
    Ok(features)
}

fn lookup(table: &[(&str, &'static str)], word: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(w, _)| *w == word)
        .map(|&(_, kana)| kana)
}

/// 内容語の読み。表、辞書（元の表記、先頭だけ大文字、小文字の順）、kanalizerの順に探す。
fn content_word_kana(
    word: &str,
    lower: &str,
    to_kana: &mut dyn FnMut(&str) -> anyhow::Result<String>,
    reanalyze: &mut dyn FnMut(&str) -> anyhow::Result<Vec<NjdFeature>>,
) -> anyhow::Result<String> {
    if let Some(kana) = lookup(CONTENT_WORDS, lower) {
        return Ok(kana.to_owned());
    }
    let capitalized = lower[..1].to_ascii_uppercase() + &lower[1..];
    for candidate in [word, &capitalized, lower] {
        if let [feature] = &*reanalyze(candidate)?
            && !matches!(&*feature.pos, "フィラー" | "記号")
        {
            return Ok(feature.pron.replace('’', ""));
        }
    }
    alphabets_to_kana(&word.replace('\'', ""), to_kana)
}

fn function_word(word: &str, kana: &str) -> NjdFeature {
    NjdFeature {
        string: word.to_owned(),
        pos: "名詞".to_owned(),
        pos_group1: "一般".to_owned(),
        pos_group2: "*".to_owned(),
        pos_group3: "*".to_owned(),
        ctype: "*".to_owned(),
        cform: "*".to_owned(),
        orig: word.to_owned(),
        read: kana.to_owned(),
        pron: kana.to_owned(),
        acc: 0,
        mora_size: split_moras(kana).len() as i32,
        chain_rule: "*".to_owned(),
        chain_flag: -1,
    }
}

/// 機能語を前に付けた内容語を、新しいアクセント句として加える。
///
/// アクセント核は内容語のものを使う。
fn push_phrase(
    features: &mut Vec<NjdFeature>,
    function_words: Vec<NjdFeature>,
    mut content: Vec<NjdFeature>,
) {
    let Some(head) = content.first_mut() else {
        return;
    };
    if function_words.is_empty() {
        head.chain_flag = 0;
        features.extend(content);
        return;
    }
    let prefix_moras = function_words.iter().map(|f| f.mora_size).sum::<i32>();
    let acc = if head.acc > 0 {
        prefix_moras + head.acc
    } else {
        0
    };
    head.chain_flag = 1;
    for (k, mut feature) in function_words.into_iter().enumerate() {
        feature.chain_flag = if k == 0 { 0 } else { 1 };
        if k == 0 {
            feature.acc = acc;
        }
        features.push(feature);
    }
    features.extend(content);
}

/// 後ろに内容語のない機能語を、前のアクセント句に付ける。前にアクセント句がなければ、それだけで
/// 外来語のアクセントのアクセント句にする。
fn flush_trailing_function_words(
    features: &mut Vec<NjdFeature>,
    mut function_words: Vec<NjdFeature>,
) {
    if function_words.is_empty() {
        return;
    }
    let attaches_to_previous = !features.is_empty();
    if !attaches_to_previous {
        let kana = function_words.iter().map(|f| &*f.pron).collect::<String>();
        function_words[0].acc = loanword_accent(&split_moras(&kana));
    }
    for (k, feature) in function_words.iter_mut().enumerate() {
        feature.chain_flag = if k == 0 && !attaches_to_previous {
            0
        } else {
            1
        };
    }
    features.extend(function_words);
}

#[cfg(test)]
mod tests {
    use open_jtalk::NjdFeature;
    use rstest::rstest;

    use super::Segment::{self, English, Text};

    fn words(words: &[&str]) -> Segment<'static> {
        English(words.iter().map(|&w| w.to_owned()).collect())
    }

    #[rstest]
    #[case("Hello, how are you today?", vec![Text("Hello, "), words(&["how", "are", "you", "today"]), Text("?")])]
    #[case("I'm fine.", vec![words(&["I'm", "fine"]), Text(".")])]
    // 全角と「’」は半角の「'」にする
    #[case("Ｉ’ｍ　ｆｉｎｅ", vec![words(&["I'm", "fine"])])]
    #[case("今日はgood morningです", vec![Text("今日は"), words(&["good", "morning"]), Text("です")])]
    // 1語だけの英単語はそのまま
    #[case("Pythonで書く", vec![Text("Pythonで書く")])]
    #[case("A B、C", vec![words(&["A", "B"]), Text("、C")])]
    // 単語の端のアポストロフィは単語に含めない
    #[case("'quoted words'", vec![Text("'"), words(&["quoted", "words"]), Text("'")])]
    fn split_english_runs_works(#[case] text: &str, #[case] expected: Vec<Segment<'_>>) {
        assert_eq!(expected, super::split_english_runs(text));
    }

    fn node(pron: &str, pos: &str, acc: i32) -> NjdFeature {
        NjdFeature {
            string: pron.to_owned(),
            pos: pos.to_owned(),
            pos_group1: "*".to_owned(),
            pos_group2: "*".to_owned(),
            pos_group3: "*".to_owned(),
            ctype: "*".to_owned(),
            cform: "*".to_owned(),
            orig: pron.to_owned(),
            read: pron.to_owned(),
            pron: pron.to_owned(),
            acc,
            mora_size: super::split_moras(pron).len() as i32,
            chain_rule: "*".to_owned(),
            chain_flag: -1,
        }
    }

    /// 辞書: 「Park」は「パーク」、カタカナは頭高の名詞1語。それ以外は未知語。
    fn fake_reanalyze(text: &str) -> anyhow::Result<Vec<NjdFeature>> {
        Ok(match text {
            "Park" => vec![node("パーク", "名詞", 1)],
            "Flat" | "フラット" => vec![node("フラット", "名詞", 0)],
            _ if text.chars().all(|c| ('ァ'..='ー').contains(&c)) => vec![node(text, "名詞", 1)],
            _ => vec![node(text, "フィラー", 0)],
        })
    }

    fn fake_to_kana(word: &str) -> anyhow::Result<String> {
        match word {
            "rust" => Ok("ラスト".to_owned()),
            "love" => Ok("ラブ".to_owned()),
            _ => anyhow::bail!("unexpected word: {word}"),
        }
    }

    /// AquesTalk風に、アクセント句を`/`で区切り、アクセント核の後ろに`'`を付ける。
    fn to_kana(features: &[NjdFeature]) -> String {
        let mut phrases = Vec::<(String, i32)>::new();
        for f in features {
            match phrases.last_mut() {
                Some((kana, _)) if f.chain_flag == 1 => kana.push_str(&f.pron),
                _ => phrases.push((f.pron.clone(), f.acc)),
            }
        }
        phrases
            .into_iter()
            .map(|(kana, acc)| {
                let moras = super::split_moras(&kana);
                let acc = usize::try_from(acc).unwrap();
                if acc == 0 {
                    return kana;
                }
                format!("{}'{}", moras[..acc].concat(), moras[acc..].concat())
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    fn english(words: &[&str]) -> String {
        let words = words.iter().map(|&w| w.to_owned()).collect::<Vec<_>>();
        let features =
            super::english_features(&words, &mut fake_to_kana, &mut fake_reanalyze).unwrap();
        to_kana(&features)
    }

    #[rstest]
    // 機能語は後ろの内容語と1つのアクセント句にし、アクセント核は内容語のものを使う
    #[case(&["to", "the", "park"], "トゥザパ'ーク")]
    #[case(&["I", "love", "Rust"], "アイラ'ブ/ラ'スト")]
    // 平板の内容語に付けても平板
    #[case(&["the", "Flat"], "ザフラット")]
    // 後ろに内容語がなければ前のアクセント句に付ける
    #[case(&["I", "love", "you"], "アイラ'ブユー")]
    // 機能語だけなら外来語のアクセント
    #[case(&["and", "you"], "アンド'ユー")]
    // 表にある内容語と句
    #[case(&["hello", "today"], "ハ'ロー/トゥ'デイ")]
    // 疑問詞は内容語
    #[case(&["how", "are", "you"], "ハ'ウアーユー")]
    #[case(&["thank", "you", "today"], "サ'ンキュー/トゥ'デイ")]
    // 辞書は先頭だけ大文字にしても引く
    #[case(&["park", "park"], "パ'ーク/パ'ーク")]
    // 大文字のみの語は1文字ずつ読む
    #[case(&["the", "NHK"], "ザエ'ヌエイチケー")]
    fn english_features_works(#[case] words: &[&str], #[case] expected: &str) {
        assert_eq!(expected, english(words));
    }
}
