//! 読みが不明な英単語の、カタカナ英語としての読み。
//!
//! VOICEVOX ENGINEの`enable_katakana_english`と同じく、MeCabが読めなかったアルファベットの語を
//! [kanalizer]でカタカナに変換する。ENGINEはアクセントを頭高に固定するが、ここでは変換した
//! カタカナを解析し直し、辞書にある外来語のアクセントや未知のカタカナ語の規則を適用する。
//!
//! [kanalizer]: https://github.com/VOICEVOX/kanalizer

use open_jtalk::NjdFeature;

/// Open JTalkがアルファベットを1文字ずつ読むときの読み。
///
/// <https://github.com/VOICEVOX/open_jtalk/blob/b9b1bf6a0cba6bc9550b4521913b20334a218dfc/src/njd_set_pronunciation/njd_set_pronunciation_rule_utf_8.h#L397>
const ALPHABET_KANAS: [&str; 26] = [
    "エー",
    "ビー",
    "シー",
    "ディー",
    "イー",
    "エフ",
    "ジー",
    "エイチ",
    "アイ",
    "ジェー",
    "ケー",
    "エル",
    "エム",
    "エヌ",
    "オー",
    "ピー",
    "キュー",
    "アール",
    "エス",
    "ティー",
    "ユー",
    "ブイ",
    "ダブリュー",
    "エックス",
    "ワイ",
    "ズィー",
];

/// 読みが不明なアルファベットの語を、カタカナ英語として解析し直したものに置き換える。
///
/// - `to_kana`: 小文字の英単語をカタカナに変換する。
/// - `reanalyze`: カタカナの文字列をNJDの特徴量に解析する。
///
/// 変換したカタカナを解析し直し、辞書にある外来語の読みとアクセントを使う。解析し直した結果に
/// 未知語などが含まれ、カタカナが不自然に区切られる場合は、1つの名詞として外来語のアクセントを
/// 付ける。`to_kana`が失敗した語は元のまま残す。
pub(super) fn convert_unknown_english(
    features: Vec<NjdFeature>,
    to_kana: &mut dyn FnMut(&str) -> anyhow::Result<String>,
    reanalyze: &mut dyn FnMut(&str) -> anyhow::Result<Vec<NjdFeature>>,
) -> anyhow::Result<Vec<NjdFeature>> {
    let mut converted = Vec::with_capacity(features.len());
    for feature in features {
        let kana = unknown_alphabets(&feature).and_then(|alphabets| {
            alphabets_to_kana(&alphabets, to_kana)
                .inspect_err(|e| tracing::warn!("could not read {alphabets:?} as English: {e}"))
                .ok()
        });
        match kana {
            Some(kana) => converted.extend(katakana_features(&feature.string, &kana, reanalyze)?),
            None => converted.push(feature),
        }
    }
    Ok(converted)
}

/// アルファベットの語をカタカナにする。キャメルケースの語は大文字の前で区切り、1文字の語と大文字のみの
/// 語は1文字ずつ読む。
pub(super) fn alphabets_to_kana(
    alphabets: &str,
    to_kana: &mut dyn FnMut(&str) -> anyhow::Result<String>,
) -> anyhow::Result<String> {
    split_into_words(alphabets)
        .map(|word| word_to_kana(word, to_kana))
        .collect()
}

/// カタカナ英語の読みを解析し直した特徴量。
///
/// 辞書にある外来語の読みとアクセントを使う。解析し直した結果に未知語などが含まれ、カタカナが不自然に
/// 区切られる場合は、1つの名詞として外来語のアクセントを付ける。
pub(super) fn katakana_features(
    string: &str,
    kana: &str,
    reanalyze: &mut dyn FnMut(&str) -> anyhow::Result<Vec<NjdFeature>>,
) -> anyhow::Result<Vec<NjdFeature>> {
    let kana = replace_vu(kana);
    let reanalyzed = reanalyze(&kana)?;
    Ok(if reanalyzed.iter().all(is_known_noun) {
        reanalyzed
    } else {
        vec![loanword_noun(string, &kana)]
    })
}

/// 「ヴォ」などを「ボ」などにして、辞書にある外来語の表記に合わせる。
fn replace_vu(kana: &str) -> String {
    const REPLACEMENTS: [(&str, &str); 5] = [
        ("ヴァ", "バ"),
        ("ヴィ", "ビ"),
        ("ヴェ", "ベ"),
        ("ヴォ", "ボ"),
        ("ヴ", "ブ"),
    ];
    REPLACEMENTS
        .iter()
        .fold(kana.to_owned(), |kana, (from, to)| kana.replace(from, to))
}

fn is_known_noun(feature: &NjdFeature) -> bool {
    feature.pos == "名詞"
}

/// 辞書にない外来語として、1つの名詞を作る。
pub(super) fn loanword_noun(string: &str, kana: &str) -> NjdFeature {
    let moras = split_moras(kana);
    NjdFeature {
        string: string.to_owned(),
        pos: "名詞".to_owned(),
        pos_group1: "固有名詞".to_owned(),
        pos_group2: "一般".to_owned(),
        pos_group3: "*".to_owned(),
        ctype: "*".to_owned(),
        cform: "*".to_owned(),
        orig: string.to_owned(),
        read: kana.to_owned(),
        pron: kana.to_owned(),
        acc: loanword_accent(&moras),
        mora_size: moras.len() as i32,
        chain_rule: "C1".to_owned(),
        chain_flag: -1,
    }
}

/// カタカナをモーラに区切る。小書きの文字は直前の文字と同じモーラにする。
pub(super) fn split_moras(kana: &str) -> Vec<&str> {
    let mut moras = Vec::<&str>::new();
    for (i, c) in kana.char_indices() {
        let end = i + c.len_utf8();
        match moras.last_mut() {
            Some(last) if "ァィゥェォャュョヮ".contains(c) => {
                *last = &kana[end - last.len() - c.len_utf8()..end];
            }
            _ => moras.push(&kana[i..end]),
        }
    }
    moras
}

/// 外来語のアクセント核の位置（1始まり）。
///
/// 後ろから3モーラ目に置き、それが長音・促音・撥音・二重母音の後半なら1つ前にずらす。
pub(super) fn loanword_accent(moras: &[&str]) -> i32 {
    let mut accent = moras.len().saturating_sub(2).max(1);
    while accent > 1 && is_special_mora(moras[accent - 1]) {
        accent -= 1;
    }
    accent as i32
}

fn is_special_mora(mora: &str) -> bool {
    matches!(mora, "ー" | "ッ" | "ン" | "イ")
}

/// 読みが不明なアルファベットのみの語であれば、そのアルファベットを半角にして返す。
fn unknown_alphabets(feature: &NjdFeature) -> Option<String> {
    // MeCabは未知語の読みを空とし、NJDは空の読みを補完して品詞をフィラーとして扱う
    if feature.pos != "フィラー" || feature.chain_rule != "*" {
        return None;
    }
    let alphabets = feature
        .string
        .chars()
        .map(to_hankaku_alphabet)
        .collect::<Option<String>>()?;
    (!alphabets.is_empty()).then_some(alphabets)
}

pub(super) fn to_hankaku_alphabet(c: char) -> Option<char> {
    match c {
        'A'..='Z' | 'a'..='z' => Some(c),
        'Ａ'..='Ｚ' | 'ａ'..='ｚ' => char::from_u32(u32::from(c) - 0xfee0),
        _ => None,
    }
}

/// キャメルケースの語に対応するため、大文字の前で区切る。
///
/// 例: `VoiceVox` → `Voice`, `Vox`、`NHK` → `N`, `H`, `K`
fn split_into_words(alphabets: &str) -> impl Iterator<Item = &str> {
    let mut rest = alphabets;
    std::iter::from_fn(move || {
        let len = rest
            .char_indices()
            .skip(1)
            .find(|(_, c)| c.is_ascii_uppercase())
            .map_or(rest.len(), |(i, _)| i);
        let (word, tail) = rest.split_at(len);
        rest = tail;
        (!word.is_empty()).then_some(word)
    })
}

/// 1文字の語と大文字のみの語は1文字ずつ読み、それ以外は英単語として読む。
fn word_to_kana(
    word: &str,
    to_kana: &mut dyn FnMut(&str) -> anyhow::Result<String>,
) -> anyhow::Result<String> {
    if word.len() == 1 || word.chars().all(|c| c.is_ascii_uppercase()) {
        return Ok(word
            .chars()
            .map(|c| ALPHABET_KANAS[usize::from(c.to_ascii_uppercase() as u8 - b'A')])
            .collect());
    }
    to_kana(&word.to_ascii_lowercase())
}

/// [kanalizer]による英単語のカタカナへの変換。
///
/// kanalizerの既定のデコード長（入力の長さ+2）では「vox」→「ヴォックス」のように入力より長い読みが
/// 途中で打ち切られるため、長めに取る。それでも終わらなかった場合は、VOICEVOX ENGINE（kanalizerの
/// Python版の既定値）と同じく途中までの読みを使う。
///
/// kanalizerの学習データ（[`DATASET`]）にある語は、モデルで推定せず表の読みを使う。
pub(super) fn kanalizer_to_kana(word: &str) -> anyhow::Result<String> {
    if let Some(kana) = dataset_kana(word) {
        return Ok(kana.to_owned());
    }
    let max_length = kanalizer::MaxLength::try_from(word.len() * 2 + 2)?;
    Ok(kanalizer::convert(word)
        .with_max_length(max_length)
        .with_error_on_incomplete(false)
        .perform()?)
}

/// kanalizerの学習データ（VOICEVOX/kanalizer-dataset v3、MIT）: 小文字の英単語とカタカナの読み、117,659語。
/// `tools/kanalizer-dataset/build.py`で作る。gzipで約0.8MB、展開は最初に引いたとき。
static DATASET: &[u8] = include_bytes!("kanalizer_dataset.tsv.gz");

fn dataset_kana(word: &str) -> Option<&'static str> {
    static TABLE: once_cell::sync::Lazy<std::collections::HashMap<&'static str, &'static str>> =
        once_cell::sync::Lazy::new(|| {
            use std::io::Read as _;
            let mut text = String::new();
            flate2::read::GzDecoder::new(DATASET)
                .read_to_string(&mut text)
                .expect("kanalizer_dataset.tsv.gz is valid gzip of UTF-8");
            let text: &'static str = text.leak();
            text.lines()
                .filter_map(|line| line.split_once('\t'))
                .collect()
        });
    TABLE.get(word).copied()
}

#[cfg(test)]
mod tests {
    use open_jtalk::NjdFeature;
    use rstest::rstest;

    fn node(string: &str, pos: &str, pron: &str, chain_rule: &str) -> NjdFeature {
        NjdFeature {
            string: string.to_owned(),
            pos: pos.to_owned(),
            pos_group1: "*".to_owned(),
            pos_group2: "*".to_owned(),
            pos_group3: "*".to_owned(),
            ctype: "*".to_owned(),
            cform: "*".to_owned(),
            orig: string.to_owned(),
            read: pron.to_owned(),
            pron: pron.to_owned(),
            acc: 0,
            mora_size: 0,
            chain_rule: chain_rule.to_owned(),
            chain_flag: -1,
        }
    }

    fn fake_to_kana(word: &str) -> anyhow::Result<String> {
        match word {
            "python" => Ok("パイソン".to_owned()),
            "voice" => Ok("ボイス".to_owned()),
            "vox" => Ok("ボックス".to_owned()),
            "phone" => Ok("フォン".to_owned()),
            _ => anyhow::bail!("unexpected word: {word}"),
        }
    }

    /// 入力をそのまま1つの名詞にする。
    fn fake_reanalyze(kana: &str) -> anyhow::Result<Vec<NjdFeature>> {
        Ok(vec![node(kana, "名詞", kana, "C1")])
    }

    fn convert(features: Vec<NjdFeature>) -> Vec<String> {
        super::convert_unknown_english(features, &mut fake_to_kana, &mut fake_reanalyze)
            .unwrap()
            .into_iter()
            .map(|f| format!("{}/{}", f.string, f.pron))
            .collect()
    }

    #[rstest]
    #[case("Ｐｙｔｈｏｎ", "パイソン/パイソン")]
    #[case("Python", "パイソン/パイソン")]
    // キャメルケースは大文字で区切る
    #[case("ＶｏｉｃｅＶｏｘ", "ボイスボックス/ボイスボックス")]
    #[case("ｉＰｈｏｎｅ", "アイフォン/アイフォン")]
    // 大文字のみの語と1文字の語は1文字ずつ読む
    #[case("ＸＹＺ", "エックスワイズィー/エックスワイズィー")]
    #[case("ａ", "エー/エー")]
    fn it_converts_unknown_alphabets(#[case] string: &str, #[case] expected: &str) {
        let features = vec![
            node(string, "フィラー", "ピーワイ", "*"),
            node("で", "助詞", "デ", "動詞%F1"),
        ];
        assert_eq!([expected, "で/デ"], *convert(features));
    }

    #[rstest]
    // 辞書にある語
    #[case(node("ｍａｃｈｉｎｅ", "名詞", "マシン", "C1"))]
    // アルファベット以外を含む未知語
    #[case(node("ＡＢ１", "フィラー", "エービーイチ", "*"))]
    // アルファベットでないフィラー
    #[case(node("えーと", "フィラー", "エート", "*"))]
    fn it_keeps_other_words(#[case] feature: NjdFeature) {
        let expected = format!("{}/{}", feature.string, feature.pron);
        assert_eq!([expected], *convert(vec![feature]));
    }

    #[test]
    fn it_keeps_words_that_cannot_be_converted() {
        let features = vec![node("Ｒｕｓｔ", "フィラー", "アールユーエスティー", "*")];
        assert_eq!(["Ｒｕｓｔ/アールユーエスティー"], *convert(features));
    }

    #[test]
    fn it_replaces_vu_before_reanalysis() {
        let features = vec![node("Ｖｏｘ", "フィラー", "ブイオーエックス", "*")];
        let mut to_kana = |_: &str| Ok("ヴォックス".to_owned());
        let actual = super::convert_unknown_english(features, &mut to_kana, &mut |kana| {
            assert_eq!("ボックス", kana);
            fake_reanalyze(kana)
        })
        .unwrap();
        assert_eq!("ボックス", actual[0].pron);
    }

    /// 解析し直した結果に未知語が含まれる場合は、1つの名詞として外来語のアクセントを付ける。
    #[test]
    fn it_falls_back_to_single_noun_when_reanalysis_is_broken() {
        let features = vec![
            node("Ｐｙｔｈｏｎ", "フィラー", "ピーワイ", "*"),
            node("で", "助詞", "デ", "動詞%F1"),
        ];
        let actual = super::convert_unknown_english(features, &mut fake_to_kana, &mut |_| {
            Ok(vec![
                node("パイ", "名詞", "パイ", "C1"),
                node("ソン", "フィラー", "ソン", "*"),
            ])
        })
        .unwrap();
        let python = &actual[0];
        assert_eq!(
            ("Ｐｙｔｈｏｎ", "名詞", "パイソン", "パイソン", 1, 4, -1),
            (
                &*python.string,
                &*python.pos,
                &*python.read,
                &*python.pron,
                python.acc,
                python.mora_size,
                python.chain_flag,
            ),
        );
        assert_eq!("で", actual[1].string);
    }

    #[rstest]
    // 後ろから3モーラ目
    #[case("クバーネテス", 4)]
    #[case("アンドロメダ", 4)]
    // 特殊モーラ（長音・促音・撥音・二重母音の後半）には置かず、1つ前にずらす
    #[case("カナライザー", 3)]
    #[case("パイソン", 1)]
    #[case("ボックス", 1)]
    #[case("ラーメン", 1)]
    // 2モーラ以下は頭高
    #[case("ガス", 1)]
    #[case("ア", 1)]
    fn loanword_accent_works(#[case] kana: &str, #[case] expected: i32) {
        assert_eq!(expected, super::loanword_accent(&super::split_moras(kana)));
    }

    #[test]
    fn it_splices_reanalyzed_features() {
        let features = vec![node("ＶｏｉｃｅＶｏｘ", "フィラー", "ブイオー", "*")];
        let actual = super::convert_unknown_english(features, &mut fake_to_kana, &mut |kana| {
            assert_eq!("ボイスボックス", kana);
            Ok(vec![
                node("ボイス", "名詞", "ボイス", "C1"),
                node("ボックス", "名詞", "ボックス", "C1"),
            ])
        })
        .unwrap();
        assert_eq!(
            ["ボイス", "ボックス"],
            *actual.iter().map(|f| &*f.pron).collect::<Vec<_>>(),
        );
    }

    /// kanalizerの学習データにある語は、モデルの推定でなくその読み（2026-10-08の聞き取りで、モデルは
    /// 「minutes」を「ミニューツ」、「eleven」を「エレベン」と読んでいた）。
    #[rstest]
    #[case("minutes", "ミニッツ")]
    #[case("eleven", "イレブン")]
    #[case("rain", "レイン")]
    #[case("umbrella", "アンブレラ")]
    fn known_words_use_the_dataset(#[case] word: &str, #[case] expected: &str) {
        assert_eq!(expected, super::kanalizer_to_kana(word).unwrap());
    }

    /// 学習データにない語はモデルで推定する。
    #[test]
    fn unknown_words_fall_back_to_the_model() {
        assert_eq!(None, super::dataset_kana("voicevoxx"));
        assert!(!super::kanalizer_to_kana("voicevoxx").unwrap().is_empty());
    }
}
