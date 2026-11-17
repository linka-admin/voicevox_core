// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)
// (`pyopenjtalk/_loanword.py`: `restore_loanword_kana`)

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
    // 結合済みの句の核は先頭の形態素が持つので、復元する語の句内の位置も追う
    let mut phrase_head = 0;
    let mut phrase_mora_offset = 0;
    for index in 0..features.len() {
        if index == 0 || features[index].chain_flag != 1 {
            phrase_head = index;
            phrase_mora_offset = 0;
        } else {
            phrase_mora_offset += features[index - 1].mora_size;
        }
        let feature = &mut features[index];
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
        let accent_shift = |acc: i32, start: i32| {
            segments
                .iter()
                .map(
                    |&Segment {
                         start: segment_start,
                         collapsed_mora_count,
                         original_mora_count,
                     }| {
                        let segment_start = start + segment_start;
                        if acc > segment_start + collapsed_mora_count {
                            collapsed_mora_count - original_mora_count
                        } else if acc > segment_start {
                            let offset_in_segment = acc - segment_start;
                            offset_in_segment - offset_in_segment.min(original_mora_count)
                        } else {
                            0
                        }
                    },
                )
                .sum::<i32>()
        };
        feature.acc -= accent_shift(feature.acc, 0);
        // 語の中の核と同じ対応を句全体の核にも適用し、縮む箇所より前にある核はそのまま保つ
        if phrase_head != index {
            let head = &mut features[phrase_head];
            head.acc -= accent_shift(head.acc, phrase_mora_offset);
        }
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
        // 表層形の「ウェイヴ」の「イ」を辞書が長音の「ウェーブ」にしたように、長音の書き方だけが
        // 違う箇所は発音の長音のまま対応させる
        if "イウ".contains(surface_char) && kana_rest.starts_with('ー') {
            restored.push('ー');
            surface_rest = &surface_rest[surface_char.len_utf8()..];
            kana_offset += 'ー'.len_utf8();
            continue;
        }
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
    // 長音の書き方だけが違う箇所は発音の長音のまま対応させる
    #[case("ウェイヴ", "ウェーブ", "ウェーヴ", 3)]
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

    /// 句の途中の語の表記を戻すと、句の先頭が持つ核も同じモーラの位置へずらす。
    #[test]
    fn restore_loanword_kana_moves_phrase_accent_nucleus() {
        // 「新イェテボリ」: 核は句頭の「新」が持ち、「テ」に置かれている
        let mut head = node("新", "接頭詞", "名詞接続", "シン");
        head.acc = 4;
        head.mora_size = 2;
        let mut word = node("イェテボリ", "名詞", "固有名詞", "イエテボリ");
        word.acc = 3;
        word.mora_size = 5;
        word.chain_flag = 1;
        let actual = super::restore_loanword_kana(vec![head, word]);
        assert_eq!((3, 2), (actual[0].acc, actual[1].acc));
    }
}
