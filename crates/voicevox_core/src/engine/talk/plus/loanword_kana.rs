// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/utils.py`: `restore_loanword_kana`)

//! 辞書が別の仮名へ置き換えた外来語の表記（「ヴィ」を「ビ」にするなど）を、表層形から戻す。

use open_jtalk::NjdFeature;

use super::kana::{is_katakana_word, mora_count};

/// 辞書が外来語表記を一般的な仮名へ置き換える組み合わせ（元の表記、置き換えた表記）。
///
/// 長い表記を先に照合し、「ヴャ」を「ヴ」だけで消費する状態を避ける。
const LOANWORD_KANA_RESTORATIONS: &[(&str, &str)] = &[
    ("ヴャ", "ビャ"),
    ("ヴュ", "ビュ"),
    ("ヴョ", "ビョ"),
    ("ヴァ", "バ"),
    ("ヴィ", "ビ"),
    ("ヴェ", "ベ"),
    ("ヴォ", "ボ"),
    ("ヴ", "ブ"),
    ("スィ", "シ"),
    ("ズィ", "ジ"),
    ("テュ", "チュ"),
    ("デュ", "ヂュ"),
    ("イェ", "イエ"),
    ("シィ", "シー"),
    ("リェ", "リエ"),
    ("ニェ", "ニエ"),
    ("ヒェ", "ヒエ"),
    ("ミェ", "ミエ"),
    ("ビェ", "ビエ"),
    ("ピェ", "ピエ"),
    ("キェ", "ケ"),
    ("ギェ", "ゲ"),
    ("グゥ", "グウ"),
    ("クゥ", "クウ"),
];

/// 表層形と読み・発音の全体が1文字ずつ対応する場合だけ戻すので、日本語で定着した別の表記や、
/// 中黒などを省いた語は変えない。
pub(super) fn restore_loanword_kana(mut features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    for feature in &mut features {
        // 外来語だけへ限定し、漢字や区切り記号を含む語の読みを表層形から作らない
        if !is_katakana_word(&feature.string) {
            continue;
        }
        if let Some(Restored { kana, .. }) = restore_spelling(&feature.string, &feature.read) {
            feature.read = kana;
        }
        let Some(Restored { kana, segments }) = restore_spelling(&feature.string, &feature.pron)
        else {
            continue;
        };
        feature.pron = kana;
        // 無声化記号はモーラに数えない
        feature.mora_size = mora_count(&feature.pron.replace('’', ""));
        // 「イエ」を「イェ」に戻すとモーラが減るので、アクセント核も戻す前と同じモーラの位置へずらす
        // 核が戻した箇所の中にあるときは、戻した箇所の最後のモーラに置く
        let mut accent_shift = 0;
        for Segment {
            start,
            collapsed_mora_count,
            original_mora_count,
        } in segments
        {
            if feature.acc > start + collapsed_mora_count {
                accent_shift += collapsed_mora_count - original_mora_count;
            } else if feature.acc > start {
                let offset_in_segment = feature.acc - start;
                accent_shift += offset_in_segment - offset_in_segment.min(original_mora_count);
            }
        }
        feature.acc -= accent_shift;
    }
    features
}

/// 外来語の表記を戻した読みまたは発音。
struct Restored {
    kana: String,
    segments: Vec<Segment>,
}

/// 表記を戻した箇所。
struct Segment {
    /// 戻す前の先頭のモーラ位置。
    start: i32,
    /// 戻す前のモーラ数。
    collapsed_mora_count: i32,
    /// 戻した後のモーラ数。
    original_mora_count: i32,
}

/// 表層形と読み（または発音）を先頭から照合し、置き換えられた外来語の仮名を表層形の表記へ戻す。
///
/// 全体が対応しないか、戻す箇所がなければ`None`。
fn restore_spelling(surface: &str, kana: &str) -> Option<Restored> {
    let mut restored = String::new();
    let mut segments = Vec::new();
    let mut surface_rest = surface;
    let mut kana_offset = 0;

    // 無声化記号は表層形に現れないため、照合位置を進めず発音側から持ち越す
    while !surface_rest.is_empty() {
        let kana_rest = &kana[kana_offset..];
        if kana_rest.starts_with('’') {
            restored.push('’');
            kana_offset += '’'.len_utf8();
            continue;
        }

        // 辞書が置き換えた組み合わせを優先し、該当しなければ同じ文字を1文字ずつ照合する
        if let Some(&(original, collapsed)) =
            LOANWORD_KANA_RESTORATIONS
                .iter()
                .find(|(original, collapsed)| {
                    surface_rest.starts_with(original) && kana_rest.starts_with(collapsed)
                })
        {
            segments.push(Segment {
                start: mora_count(&kana[..kana_offset].replace('’', "")),
                collapsed_mora_count: mora_count(collapsed),
                original_mora_count: mora_count(original),
            });
            restored += original;
            surface_rest = &surface_rest[original.len()..];
            kana_offset += collapsed.len();
            continue;
        }

        let surface_char = surface_rest.chars().next().expect("not empty");
        if !kana_rest.starts_with(surface_char) {
            return None;
        }
        restored.push(surface_char);
        surface_rest = &surface_rest[surface_char.len_utf8()..];
        kana_offset += surface_char.len_utf8();
    }

    // 語末の無声化記号も、表層形との対応を崩さず保持する
    if kana[kana_offset..].starts_with('’') {
        restored.push('’');
        kana_offset += '’'.len_utf8();
    }
    (kana_offset == kana.len() && !segments.is_empty()).then_some(Restored {
        kana: restored,
        segments,
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::super::test_util::node;

    #[rstest]
    #[case("ヴィクトリーヌ", "ビク’トリーヌ", "ヴィク’トリーヌ", 6)]
    #[case("アイシュヴァルヤ", "アイシュバルヤ", "アイシュヴァルヤ", 6)]
    #[case("テュルク", "チュルク", "テュルク", 3)]
    #[case("アクスィス", "アクシス", "アクスィス", 4)]
    fn restore_loanword_kana_works(
        #[case] surface: &str,
        #[case] pron: &str,
        #[case] expected: &str,
        #[case] expected_mora_size: i32,
    ) {
        let [actual] = <[_; 1]>::try_from(super::restore_loanword_kana(vec![node(
            surface,
            "名詞",
            "固有名詞",
            pron,
        )]))
        .unwrap();
        assert_eq!(
            (expected, expected, expected_mora_size),
            (&*actual.read, &*actual.pron, actual.mora_size),
        );
    }

    #[rstest]
    #[case("ホンデュラス", "ホンジュラス")]
    #[case("バースディ", "バースデイ")]
    #[case("キウィ", "キウイ")]
    #[case("エヌ・エイチ・ヴィ", "エヌエイチブイ")]
    fn restore_loanword_kana_keeps_unmatched_pronunciation(
        #[case] surface: &str,
        #[case] pron: &str,
    ) {
        let input = node(surface, "名詞", "固有名詞", pron);
        let actual = super::restore_loanword_kana(vec![input.clone()]);
        assert_eq!(vec![input], actual);
    }

    /// 「イエ」を「イェ」に戻してモーラが減っても、アクセント核を同じ「テ」のモーラに置く。
    #[test]
    fn restore_loanword_kana_moves_accent_nucleus() {
        let mut input = node("イェテボリ", "名詞", "固有名詞", "イエテボリ");
        input.acc = 3;
        input.mora_size = 5;
        let [actual] = <[_; 1]>::try_from(super::restore_loanword_kana(vec![input])).unwrap();
        assert_eq!(
            ("イェテボリ", 2, 4),
            (&*actual.pron, actual.acc, actual.mora_size),
        );
    }
}
