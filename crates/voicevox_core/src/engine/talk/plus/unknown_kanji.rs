// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/utils.py`: `read_unknown_kanji`)

//! 辞書にないためNJDが読みを付けられなかった漢字に、Unihanの音読みを付ける。
//!
//! pyopenjtalk-plusは送り仮名を伴う語（「悪魔憑き」など）にSudachiの語としての読みも使うが、
//! ここではUnihanの漢字ごとの音読みだけを使う。

use std::{collections::HashMap, sync::LazyLock};

use open_jtalk::NjdFeature;

use super::kana::{mora_count, split_kana_mora};

/// UnihanのkJapaneseによる漢字ごとの音読み（`UNIHAN_READINGS`）。
///
/// `unihan_readings.txt`は読みごとに漢字をまとめたもので、`#`で始まる行は注釈。
static UNIHAN_READINGS: LazyLock<HashMap<char, &'static str>> = LazyLock::new(|| {
    include_str!("unihan_readings.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .flat_map(|line| {
            let (reading, kanji) = line.split_once('\t').expect("should be tab-separated");
            kanji.chars().map(move |c| (c, reading))
        })
        .collect()
});

/// NJDが記号・読点に変えた漢字のうち、すべての文字に一意なUnihanの読みがあるものに読みを付ける。
pub(super) fn read_unknown_kanji(mut features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    for feature in &mut features {
        // NJDが記号・読点に変えた漢字だけを対象にし、本物の句読点と既知の語は変えない
        if feature.pos != "記号"
            || feature.pos_group1 != "読点"
            || !feature.string.chars().any(is_kanji)
        {
            continue;
        }
        let Some(reading) = unihan_reading(&feature.string) else {
            continue;
        };

        feature.pron = reading_to_pronunciation(&reading);
        feature.read = reading;
        feature.pos = "名詞".to_owned();
        feature.pos_group1 = "一般".to_owned();
        feature.pos_group2 = "*".to_owned();
        feature.pos_group3 = "*".to_owned();
        feature.mora_size = mora_count(&feature.pron);
        feature.acc = 0;
        feature.chain_flag = -1;
    }
    features
}

/// すべての漢字にUnihanの読みがある場合だけ、漢字を読みに置き換えた文字列を返す。
fn unihan_reading(surface: &str) -> Option<String> {
    surface
        .chars()
        .map(|c| match UNIHAN_READINGS.get(&c) {
            Some(reading) => Some((*reading).to_owned()),
            None if !is_kanji(c) => Some(c.to_string()),
            None => None,
        })
        .collect()
}

/// Unihanの読みを付ける対象になるCJK統合漢字（拡張A以降と互換漢字を含む）か。
fn is_kanji(c: char) -> bool {
    matches!(
        u32::from(c),
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3FFFF
    )
}

/// 読みの連母音（「オウ」「エイ」など）を、Open JTalkの発音の長音「ー」に書き換える。
fn reading_to_pronunciation(reading: &str) -> String {
    let mut pronunciation = String::new();
    let mut previous_vowel = None;
    for mora in split_kana_mora(reading) {
        if matches!(
            (previous_vowel, mora),
            (Some('o'), "ウ" | "オ") | (Some('u'), "ウ") | (Some('e'), "イ")
        ) {
            pronunciation.push('ー');
            continue;
        }
        pronunciation += mora;
        if mora != "ー" {
            previous_vowel = mora.chars().last().and_then(vowel_of);
        }
    }
    pronunciation
}

/// 仮名の最後の文字から母音を引く表（`_VOWEL_BY_LAST_KANA`）。
fn vowel_of(kana: char) -> Option<char> {
    [
        ("アカサタナハマヤラワガザダバパャァ", 'a'),
        ("イキシチニヒミリヰギジヂビピィ", 'i'),
        ("ウクスツヌフムユルグズヅブプヴュゥ", 'u'),
        ("エケセテネヘメレヱゲゼデベペェ", 'e'),
        ("オコソトノホモヨロヲゴゾドボポョォ", 'o'),
    ]
    .into_iter()
    .find_map(|(kanas, vowel)| kanas.contains(kana).then_some(vowel))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::super::test_util::{node, parse_nodes};

    #[rstest]
    #[case("騸", "セン", "セン", 2)]
    #[case("痘", "トウ", "トー", 2)]
    #[case("嗅", "キュウ", "キュー", 2)]
    #[case("嚳", "コク", "コク", 2)]
    fn read_unknown_kanji_works(
        #[case] surface: &str,
        #[case] read: &str,
        #[case] pron: &str,
        #[case] mora_size: i32,
    ) {
        let mut input = node(surface, "記号", "読点", "、");
        input.chain_flag = 0;
        input.pos_group2 = "x".to_owned();
        let [actual] = <[_; 1]>::try_from(super::read_unknown_kanji(vec![input])).unwrap();
        assert_eq!(
            (read, pron, "名詞", "一般", "*", "*", mora_size, 0, -1),
            (
                &*actual.read,
                &*actual.pron,
                &*actual.pos,
                &*actual.pos_group1,
                &*actual.pos_group2,
                &*actual.pos_group3,
                actual.mora_size,
                actual.acc,
                actual.chain_flag,
            ),
        );
    }

    /// 本物の句読点、読みのない漢字を含む語、既知の語は変えない。
    #[test]
    fn read_unknown_kanji_keeps_others() {
        let input = parse_nodes(
            "、 記号 読点 * * 、 、 、 0 0 * 0
             々 記号 読点 * * 々 、 、 0 0 * 0
             馬 名詞 一般 * * 馬 ウマ ウマ 2 2 C3 -1",
        );
        assert_eq!(input.clone(), super::read_unknown_kanji(input));
    }
}
