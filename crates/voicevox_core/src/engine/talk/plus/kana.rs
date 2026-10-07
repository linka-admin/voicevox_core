// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/_kana_utils.py`)

//! 仮名の文字種の判定。

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

#[cfg(test)]
mod tests {
    use rstest::rstest;

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
}
