// Based on pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)
// (`pyopenjtalk/__init__.py`: `MULTI_READ_KANJI_LIST`, `pyopenjtalk/utils.py`: `modify_kanji_yomi`)

//! 読みが複数ある漢字1文字の形態素の読み（pyopenjtalk-plusの`modify_kanji_yomi`の代わり）。
//!
//! pyopenjtalk-plusは`use_sudachi_kanji_yomi=True`（既定）で、これらの漢字1文字の形態素の読みと
//! 発音をSudachi（SudachiDict-core）の読みで上書きする。Sudachiの辞書はアプリに載せるには大きいため、
//! ここではpyopenjtalk-plus v0.4.1-post10の出力（Tatoebaの文でSudachiあり・なしを比べたもの）から
//! 読み取った規則で同じ結果を作る。
//!
//! - 読みを変えない形態素も、発音を読みの仮名で上書きする（Sudachiの読みは長音にしない仮名なので、
//!   「低」は「テー」でなく「テイ」になる。「方」の「ホウ」だけは発音を「ホー」にする）。発音から
//!   無声化記号が消えるので、無声化は[`super::devoicing`]が判定し直す。
//! - 前後の語で読みが決まる語（「この方」のカタ、「こんな風」のフウ、「この種の」のシュ、「主」の
//!   ヌシ、「値」のアタイなど）は[`decide`]の規則で読みを変える。
//! - Sudachiが接頭辞を後ろの語とまとめて1語にすると、pyopenjtalk-plusは漢字と読みを対応付けられず、
//!   文全体で補正をしない。規則の当たらない接頭辞の対象漢字がある文は、それに倣って何もしない。
//!
//! Sudachiが統計的に選ぶ読みのうち、文脈の規則にならないもの（金銭の「金」をキンと読む、
//! 「堪忍袋の緒」をイトグチと読むなど）は再現しない。

use open_jtalk::NjdFeature;

use super::kana::mora_count;

/// pyopenjtalk-plusの`MULTI_READ_KANJI_LIST`から「何」を除いたもの。
const MULTI_READ_KANJI: &[char] = &[
    '風', '観', '方', '出', '時', '上', '下', '君', '手', '嫌', '表', '対', '色', '人', '前', '後',
    '角', '金', '頭', '筆', '水', '間', '棚', '床', '入', '来', '塗', '怒', '包', '被', '開', '弾',
    '捻', '潜', '支', '抱', '行', '降', '種', '訳', '糞', '空', '性', '体', '等', '生', '止', '堪',
    '捩', '家', '縁', '労', '中', '高', '低', '気', '要', '退', '面', '主', '術', '直', '片', '緒',
    '小', '大', '値',
];

/// 指示の連体詞。
const DEMONSTRATIVES: &[&str] = &["この", "その", "あの", "どの"];

/// モーラ数の変わる読みに替えるときに、アクセント核を計算し直すための辞書の行（MeCabの特徴量）。
///
/// pyopenjtalk-plusは同じ読みの辞書の行のうち、品詞が同じもの、なければコストの最も低いものを選ぶ。
const DICTIONARY_ENTRIES: &[(&str, &str, &str)] = &[
    ("主", "ヌシ", "主,名詞,一般,*,*,*,*,主,ヌシ,ヌシ,1/2,C3"),
    ("種", "シュ", "種,名詞,一般,*,*,*,*,種,シュ,シュ,1/1,C3"),
    (
        "値",
        "アタイ",
        "値,名詞,一般,*,*,*,*,値,アタイ,アタイ,0/3,C2",
    ),
    (
        "表",
        "ヒョウ",
        "表,名詞,一般,*,*,*,*,表,ヒョウ,ヒョー,0/2,C4",
    ),
];

/// 読みが複数ある漢字1文字の形態素の読みを、pyopenjtalk-plusがSudachiで決める読みに合わせる。
///
/// `phrase_accent`はMeCabの特徴量の列をNJDの全段階で解析し、先頭のアクセント核を返す
/// （pyopenjtalk-plusの`OpenJTalk.run_njd_from_mecab`）。モーラ数の変わる読みに替えた句頭の
/// アクセント核の計算に使う。
pub(super) fn modify_kanji_yomi(
    mut features: Vec<NjdFeature>,
    phrase_accent: &mut dyn FnMut(&[String]) -> anyhow::Result<i32>,
) -> anyhow::Result<Vec<NjdFeature>> {
    let targets = (0..features.len())
        .filter(|&i| is_target(&features[i]))
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Ok(features);
    }
    let decisions = targets
        .iter()
        .map(|&i| (i, decide(&features, i)))
        .collect::<Vec<_>>();
    // Sudachiが接頭辞を後ろの語とまとめると対応付けに失敗し、文全体で何もしない
    if decisions
        .iter()
        .any(|&(i, reading)| features[i].pos == "接頭詞" && reading.is_none())
    {
        return Ok(features);
    }

    // pyopenjtalk-plusと同じく後ろから順に書き換え、句のアクセント核の計算に書き換えた後の語を使う
    for &(i, reading) in decisions.iter().rev() {
        // NJDが接尾辞として決めた読みは、前の語との結合を反映したものなので保つ
        if features[i].pos_group1 == "接尾" {
            continue;
        }
        let Some(reading) = reading else {
            let feature = &mut features[i];
            feature.pron = if feature.orig == "方" && feature.read == "ホウ" {
                "ホー".to_owned()
            } else {
                feature.read.clone()
            };
            feature.mora_size = mora_count(&feature.pron);
            continue;
        };
        let mut pron = reading.to_owned();
        if features[i].read != reading
            && features[i].mora_size != mora_count(reading)
            && features[i].chain_flag != 1
            && let Some(&(_, _, entry)) =
                DICTIONARY_ENTRIES.iter().find(|(kanji, entry_reading, _)| {
                    *kanji == features[i].orig && *entry_reading == reading
                })
        {
            // 句頭の読みを替えてモーラ数が変わるときは、新しい辞書の行と後ろに結合した語でNJDに
            // アクセント核を計算させる（「オモテ」の核3を「ヒョウ」に残すと後ろの助詞で下がる）
            pron = entry.split(',').nth(9).expect("10 columns").to_owned();
            let phrase = std::iter::once(entry.to_owned())
                .chain(
                    features[i + 1..]
                        .iter()
                        .take_while(|f| f.chain_flag == 1)
                        .map(to_mecab_feature),
                )
                .collect::<Vec<_>>();
            features[i].acc = phrase_accent(&phrase)?;
        }
        let feature = &mut features[i];
        feature.read = reading.to_owned();
        feature.mora_size = mora_count(&pron);
        feature.pron = pron;
    }
    Ok(features)
}

fn is_target(feature: &NjdFeature) -> bool {
    let mut chars = feature.orig.chars();
    matches!((chars.next(), chars.next()), (Some(kanji), None) if MULTI_READ_KANJI.contains(&kanji))
}

/// 前後の語から決まる読み。規則が当たらなければ`None`（辞書の読みのまま）。
fn decide(features: &[NjdFeature], i: usize) -> Option<&'static str> {
    let at = |offset: isize| {
        i.checked_add_signed(offset)
            .and_then(|j| features.get(j))
            .map_or("", |f| &*f.string)
    };
    let (previous, next) = (at(-1), at(1));
    let feature = &features[i];
    match (&*feature.orig, &*feature.read) {
        // 「この方」「小野さんという方が」は人を指すカタ
        ("方", "ホウ")
            if DEMONSTRATIVES.contains(&previous)
                || (["いう", "言う"].contains(&previous)
                    && [
                        "が", "は", "に", "を", "の", "です", "で", "も", "と", "から",
                    ]
                    .contains(&next)) =>
        {
            Some("カタ")
        }
        // 「こんな風に」は様子のフウ
        ("風", "カゼ") if ["こんな", "そんな", "あんな", "どんな"].contains(&previous) => {
            Some("フウ")
        }
        // 「この種の」「ある種の」「種の起源」は種類のシュ
        ("種", "タネ")
            if next == "の"
                && (DEMONSTRATIVES.contains(&previous)
                    || ["ある", "各"].contains(&previous)
                    || at(2) == "起源") =>
        {
            Some("シュ")
        }
        // 「値」はアタイ。慣用句の「値が張る」はネのまま
        ("値", "ネ") if !(next == "が" && at(2).starts_with('張')) => Some("アタイ"),
        // 「主な」「主に」以外の「主」と、「アルジ」と読んだ「主」はヌシ
        ("主", "オモ") if !["な", "に"].contains(&next) => Some("ヌシ"),
        ("主", "アルジ") => Some("ヌシ"),
        // 「高そう」「高すぎ」は形容詞のタカ
        ("高", "コウ") if next.starts_with("そう") || next.starts_with("すぎ") => {
            Some("タカ")
        }
        // 「この表」「次の表」は図表のヒョウ
        ("表", "オモテ")
            if DEMONSTRATIVES.contains(&previous)
                || (previous == "の"
                    && ["次", "下記", "上記", "以下", "左", "右", "前"].contains(&at(-2))) =>
        {
            Some("ヒョウ")
        }
        // 「星空の下、」はモト
        ("下", "シタ") if previous == "の" && next == "、" => Some("モト"),
        // 「戦いの後ナポレオンは」のように接頭辞と解析された「後」はアト
        ("後", "コウ") if feature.pos == "接頭詞" && ["の", "た", "だ"].contains(&previous) => {
            Some("アト")
        }
        ("糞", "フン") => Some("クソ"),
        _ => None,
    }
}

/// pyopenjtalk-plusの`modify_kanji_yomi`がNJDに渡し直す形のMeCabの特徴量。
fn to_mecab_feature(f: &NjdFeature) -> String {
    [
        &*f.string,
        &f.pos,
        &f.pos_group1,
        &f.pos_group2,
        &f.pos_group3,
        &f.ctype,
        &f.cform,
        &f.orig,
        &f.read,
        &f.pron,
        &format!("{}/{}", f.acc, f.mora_size),
        &f.chain_rule,
        &f.chain_flag.to_string(),
    ]
    .join(",")
}

#[cfg(test)]
mod tests {
    use open_jtalk::NjdFeature;
    use rstest::rstest;

    use super::super::test_util::parse_nodes;

    fn apply(features: Vec<NjdFeature>) -> Vec<NjdFeature> {
        super::modify_kanji_yomi(features, &mut |_| panic!("should not be called")).unwrap()
    }

    fn readings(features: &[NjdFeature]) -> Vec<(&str, &str)> {
        features.iter().map(|f| (&*f.read, &*f.pron)).collect()
    }

    #[rstest]
    // この方
    #[case(
        "この 連体詞 * * * この コノ コノ 0 2 * -1
         方 名詞 非自立 * * 方 ホウ ホー 1 2 C3 1",
        &[("コノ", "コノ"), ("カタ", "カタ")],
    )]
    // 読みを変えない「方」の発音はホー
    #[case(
        "た 助動詞 * 特殊・タ 基本形 た タ タ 0 1 * -1
         方 名詞 非自立 * * 方 ホウ ホウ 1 2 C3 1",
        &[("タ", "タ"), ("ホウ", "ホー")],
    )]
    // こんな風
    #[case(
        "こんな 連体詞 * * * こんな コンナ コンナ 0 3 * -1
         風 名詞 一般 * * 風 カゼ カゼ 0 2 C1 0",
        &[("コンナ", "コンナ"), ("フウ", "フウ")],
    )]
    // 主に、主よ
    #[case(
        "主 名詞 形容動詞語幹 * * 主 オモ オモ 1 2 C1 -1
         に 助詞 副詞化 * * に ニ ニ 0 1 * 1
         主 名詞 形容動詞語幹 * * 主 オモ オモ 1 2 C1 0
         よ 助詞 終助詞 * * よ ヨ ヨ 0 1 * 1",
        &[("オモ", "オモ"), ("ニ", "ニ"), ("ヌシ", "ヌシ"), ("ヨ", "ヨ")],
    )]
    // 値が張る
    #[case(
        "値 名詞 一般 * * 値 ネ ネ 0 1 C1 -1
         が 助詞 格助詞 * * が ガ ガ 0 1 * 1
         張る 動詞 自立 * * 張る ハル ハル 0 2 * 0",
        &[("ネ", "ネ"), ("ガ", "ガ"), ("ハル", "ハル")],
    )]
    // 発音を読みの仮名で上書きし、無声化記号と長音を外す
    #[case(
        "人 名詞 一般 * * 人 ヒト ヒ’ト 0 2 C1 -1
         性 名詞 一般 * * 性 セイ セー 1 2 C1 0",
        &[("ヒト", "ヒト"), ("セイ", "セイ")],
    )]
    // 接尾辞は変えない
    #[case(
        "日本 名詞 固有名詞 * * 日本 ニッポン ニッポン 3 4 C1 -1
         人 名詞 接尾 * * 人 ジン ジン 1 2 C3 1",
        &[("ニッポン", "ニッポン"), ("ジン", "ジン")],
    )]
    // 規則の当たらない接頭辞の対象漢字がある文は、Sudachiと対応付けられないので何もしない
    #[case(
        "低 接頭詞 名詞接続 * * 低 テイ テー 1 2 P1 -1
         金利 名詞 一般 * * 金利 キンリ キンリ 0 3 C1 1
         の 助詞 連体化 * * の ノ ノ 0 1 * 1
         方 名詞 非自立 * * 方 ホウ ホウ 1 2 C3 0",
        &[("テイ", "テー"), ("キンリ", "キンリ"), ("ノ", "ノ"), ("ホウ", "ホウ")],
    )]
    fn modify_kanji_yomi_works(#[case] features: &str, #[case] expected: &[(&str, &str)]) {
        assert_eq!(expected, readings(&apply(parse_nodes(features))));
    }

    #[test]
    fn it_recomputes_phrase_accent_when_mora_count_changes() {
        // この表には: 「オモテ」の核3を「ヒョウ」に残さず、辞書の行と後ろの助詞で計算し直す
        let features = parse_nodes(
            "
            この 連体詞 * * * この コノ コノ 0 2 * -1
            表 名詞 一般 * * 表 オモテ オモテ 3 3 C2 0
            に 助詞 格助詞 * * に ニ ニ 0 1 * 1
            は 助詞 係助詞 * * は ハ ワ 0 1 * 1
            ",
        );
        let mut phrases = vec![];
        let actual = super::modify_kanji_yomi(features, &mut |phrase| {
            phrases.push(phrase.to_vec());
            Ok(0)
        })
        .unwrap();
        assert_eq!(
            vec![vec![
                "表,名詞,一般,*,*,*,*,表,ヒョウ,ヒョー,0/2,C4".to_owned(),
                "に,助詞,格助詞,*,*,*,*,に,ニ,ニ,0/1,*,1".to_owned(),
                "は,助詞,係助詞,*,*,*,*,は,ハ,ワ,0/1,*,1".to_owned(),
            ]],
            phrases,
        );
        assert_eq!(
            ("ヒョウ", "ヒョー", 0, 2),
            (
                &*actual[1].read,
                &*actual[1].pron,
                actual[1].acc,
                actual[1].mora_size
            ),
        );
    }
}
