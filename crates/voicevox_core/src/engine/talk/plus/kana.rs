// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/_kana_utils.py`, `pyopenjtalk/utils.py`)

//! 仮名の文字種の判定とモーラへの分割。

/// 前の文字と結合して1モーラになる小書き仮名。
pub(super) const SMALL_KANA: &str = "ャュョァィゥェォ";

/// 1文字以上あり、すべてカタカナ（長音符と踊り字「ヽ」「ヾ」を含む）か。
pub(super) fn is_katakana_word(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| ('ァ'..='ヴ').contains(&c) || "ーヽヾ".contains(c))
}

/// ひらがなを1文字でも含むか。
pub(super) fn contains_hiragana(text: &str) -> bool {
    text.chars().any(|c| ('ぁ'..='ゖ').contains(&c))
}

/// 仮名の段。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Dan {
    A,
    I,
    U,
    E,
    O,
}

/// カタカナ1文字の段（`_DAN_MAP`）。
pub(super) fn dan(c: char) -> Option<Dan> {
    match c {
        'ア' | 'カ' | 'サ' | 'タ' | 'ナ' | 'ハ' | 'マ' | 'ヤ' | 'ラ' | 'ワ' | 'ガ' | 'ザ'
        | 'ダ' | 'バ' | 'パ' | 'ァ' => Some(Dan::A),
        'イ' | 'キ' | 'シ' | 'チ' | 'ニ' | 'ヒ' | 'ミ' | 'リ' | 'ギ' | 'ジ' | 'ヂ' | 'ビ'
        | 'ピ' | 'ィ' => Some(Dan::I),
        'ウ' | 'ク' | 'ス' | 'ツ' | 'ヌ' | 'フ' | 'ム' | 'ユ' | 'ル' | 'グ' | 'ズ' | 'ヅ'
        | 'ブ' | 'プ' | 'ヴ' | 'ゥ' => Some(Dan::U),
        'エ' | 'ケ' | 'セ' | 'テ' | 'ネ' | 'ヘ' | 'メ' | 'レ' | 'ゲ' | 'ゼ' | 'デ' | 'ベ'
        | 'ペ' | 'ェ' => Some(Dan::E),
        'オ' | 'コ' | 'ソ' | 'ト' | 'ノ' | 'ホ' | 'モ' | 'ヨ' | 'ロ' | 'ヲ' | 'ゴ' | 'ゾ'
        | 'ド' | 'ボ' | 'ポ' | 'ォ' => Some(Dan::O),
        _ => None,
    }
}

/// 仮名の文字列をモーラ単位に分割する（`split_kana_mora`）。
///
/// 小書き仮名（ャュョァィゥェォ）は前の文字と結合して1モーラとして扱う。
pub(super) fn split_kana_mora(text: &str) -> Vec<&str> {
    let mut moras = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, _)) = chars.next() {
        if chars.peek().is_some_and(|&(_, c)| SMALL_KANA.contains(c)) {
            chars.next();
        }
        let end = chars.peek().map_or(text.len(), |&(i, _)| i);
        moras.push(&text[start..end]);
    }
    moras
}

/// [`split_kana_mora`]によるモーラ数。
pub(super) fn mora_count(text: &str) -> i32 {
    split_kana_mora(text).len() as _
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::Dan;

    #[rstest]
    #[case("ヌメロワール", true)]
    #[case("ヴァーヽ", true)]
    #[case("", false)]
    #[case("カタかな", false)]
    #[case("ABC", false)]
    fn is_katakana_word_works(#[case] text: &str, #[case] expected: bool) {
        assert_eq!(expected, super::is_katakana_word(text));
    }

    #[rstest]
    #[case("投げ", true)]
    #[case("野球", false)]
    #[case("", false)]
    fn contains_hiragana_works(#[case] text: &str, #[case] expected: bool) {
        assert_eq!(expected, super::contains_hiragana(text));
    }

    #[rstest]
    #[case("", &[])]
    #[case("キョー", &["キョ", "ー"])]
    #[case("ジョジョ", &["ジョ", "ジョ"])]
    #[case("ァァ", &["ァァ"])]
    #[case("シュギ", &["シュ", "ギ"])]
    fn split_kana_mora_works(#[case] text: &str, #[case] expected: &[&str]) {
        assert_eq!(expected, super::split_kana_mora(text));
    }

    #[rstest]
    #[case('ナ', Some(Dan::A))]
    #[case('ヂ', Some(Dan::I))]
    #[case('ヴ', Some(Dan::U))]
    #[case('ェ', Some(Dan::E))]
    #[case('ヲ', Some(Dan::O))]
    #[case('ン', None)]
    #[case('ー', None)]
    fn dan_works(#[case] c: char, #[case] expected: Option<Dan>) {
        assert_eq!(expected, super::dan(c));
    }
}
