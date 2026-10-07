// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/__init__.py`: `modify_filler_accent`,
// `pyopenjtalk/utils.py`: `split_prefix_accent_phrase`, `retreat_acc_nuc`,
// `modify_acc_after_chaining`)

//! アクセント句の区切りとアクセント核の後処理。

use open_jtalk::NjdFeature;

use super::kana::{Dan, SMALL_KANA, dan};

/// 後ろの語を独立したアクセント句として読む、指示的な漢語の接頭辞（「本論文」の「本」など）。
const INDEPENDENT_PREFIXES: &[&str] = &["本", "当", "同", "全"];

/// アクセント核を置けない特殊拍。
const INAPPROPRIATE_FOR_NUCLEUS: &[char] = &['ー', 'ッ', 'ン'];

/// アクセント句の先頭（`chain_flag`が`0`か`-1`）か。先頭のノードがアクセント核の位置を持つ。
fn is_phrase_head(feature: &NjdFeature) -> bool {
    matches!(feature.chain_flag, 0 | -1)
}

/// フィラーのアクセント核を補正し、直後の名詞が前のアクセント句へ連結されるのを防ぐ。
pub(super) fn modify_filler_accent(mut features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    let mut is_after_filler = false;
    for feature in &mut features {
        if feature.pos == "フィラー" {
            if feature.acc > feature.mora_size {
                feature.acc = 0;
            }
            is_after_filler = true;
        } else if is_after_filler {
            if feature.pos == "名詞" {
                feature.chain_flag = 0;
            }
            is_after_filler = false;
        }
    }
    features
}

/// 「本論文」「当ホテル」のような指示的な漢語の接頭辞の後ろの語を、独立したアクセント句にする。
pub(super) fn split_prefix_accent_phrase(mut features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    for i in 1..features.len() {
        let (previous, current) = (&features[i - 1], &features[i]);
        if current.chain_flag == 1
            && previous.pos == "接頭詞"
            && INDEPENDENT_PREFIXES.contains(&&*previous.string)
        {
            features[i].chain_flag = 0;
        }
    }
    features
}

/// 特殊拍上のアクセント核と、前部末型の結合で準特殊拍に来た核を1モーラ前へずらす。
///
/// 準特殊拍はア段に続く「イ」と、発音に無声化記号が付いた拍を対象とする。NHKアクセント辞典の
/// 「警戒心」「仙台市」と同じ補正を、C3で結合する語に適用する。
pub(super) fn retreat_acc_nuc(mut features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    let mut head = 0;
    let mut acc = 0;

    for index in 0..features.len() {
        if is_phrase_head(&features[index]) {
            head = index;
            acc = features[index].acc;
        }
        if acc <= 0 {
            continue;
        }
        let feature = &features[index];
        if acc > feature.mora_size {
            acc -= feature.mora_size;
            continue;
        }

        let mut pron = feature
            .pron
            .chars()
            .filter(|&c| !SMALL_KANA.contains(c))
            .collect::<Vec<_>>();
        if pron.is_empty() {
            pron = feature.pron.chars().collect();
        }
        // C3が置いた前部末の核だけ、二重母音と無声化拍の補正も適用する
        // 「クイ」などの母音連続は仮名だけでは二重母音と確定できないため、ア段＋イに限定する
        let is_c3_boundary = acc == feature.mora_size
            && features
                .get(index + 1)
                .is_some_and(|next| next.chain_flag == 1 && next.chain_rule == "C3");
        // 無声化記号を拍数から除き、前部末の拍とその直前の母音を調べる
        if is_c3_boundary {
            pron.retain(|&c| c != '’');
        }
        // 核がモーラ数を超える場合は先頭の拍を見る
        let Some(&nucleus) = pron.get(acc as usize - 1).or(pron.first()) else {
            acc = -1;
            continue;
        };
        let previous_kana = {
            let chars = feature
                .pron
                .chars()
                .filter(|&c| c != '’')
                .collect::<Vec<_>>();
            (chars.len() >= 2).then(|| chars[chars.len() - 2])
        };
        let is_quasi_special_mora = (pron.len() >= 2
            && nucleus == 'イ'
            && previous_kana.is_some_and(|c| dan(c) == Some(Dan::A) || c == 'ャ'))
            || feature.pron.ends_with('’');
        if INAPPROPRIATE_FOR_NUCLEUS.contains(&nucleus) || (is_c3_boundary && is_quasi_special_mora)
        {
            features[head].acc -= 1;
        }
        acc = -1;
    }
    features
}

/// 「特殊・マス」などが核のある動詞に続く場合に、アクセント核を移す。
///
/// 書きます → か[きま]す、参ります → ま[いりま]す、書いております → [か]いております
pub(super) fn modify_acc_after_chaining(mut features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    let mut acc = 0;
    let mut is_after_nucleus = false;
    let mut phrase_len = 0;
    let mut head = 0;

    for index in 0..features.len() {
        let feature = &features[index];
        if is_phrase_head(feature) {
            is_after_nucleus = false;
            head = index;
            acc = feature.acc;
            phrase_len = 0;
        }
        // 平板の句には「特殊・マス」の補正は要らない
        if acc == 0 {
            continue;
        }
        let mora_size = feature.mora_size;
        if is_after_nucleus {
            if feature.ctype == "特殊・マス" {
                features[head].acc = if feature.cform != "未然形" {
                    phrase_len + 1
                } else {
                    phrase_len + 2
                };
            } else if feature.ctype == "特殊・ナイ" {
                features[head].acc = phrase_len;
            } else if ["れる", "られる", "すぎる", "せる", "させる"].contains(&&*feature.orig)
            {
                features[head].acc = phrase_len + feature.acc;
            } else {
                is_after_nucleus = false;
                acc = 0;
            }
            phrase_len += mora_size;
        } else {
            phrase_len += mora_size;
            if acc <= mora_size {
                is_after_nucleus = true;
            } else {
                acc -= mora_size;
            }
        }
    }
    features
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::super::test_util::parse_nodes;

    fn accents_and_chain_flags(features: &[open_jtalk::NjdFeature]) -> Vec<(i32, i32)> {
        features.iter().map(|f| (f.acc, f.chain_flag)).collect()
    }

    #[rstest]
    // 句の長さを超える核を平板にし、直後の名詞を新しいアクセント句にする
    #[case(
        "えーと フィラー * * * えーと エート エート 5 3 C1 -1
         東京 名詞 固有名詞 * * 東京 トウキョウ トーキョー 0 4 C2 1",
        &[(0, -1), (0, 0)],
    )]
    // 名詞以外は連結を変えない
    #[case(
        "あの フィラー * * * あの アノ アノ 0 2 * -1
         ね 助詞 終助詞 * * ね ネ ネ 0 1 * 1",
        &[(0, -1), (0, 1)],
    )]
    fn modify_filler_accent_works(#[case] features: &str, #[case] expected: &[(i32, i32)]) {
        let actual = super::modify_filler_accent(parse_nodes(features));
        assert_eq!(expected, accents_and_chain_flags(&actual));
    }

    #[rstest]
    #[case(
        "本 接頭詞 名詞接続 * * 本 ホン ホン 1 1 P1 -1
         論文 名詞 一般 * * 論文 ロンブン ロンブン 0 4 C1 1",
        &[(1, -1), (0, 0)],
    )]
    // 語彙的に融合する接頭辞はそのまま
    #[case(
        "新 接頭詞 名詞接続 * * 新 シン シン 1 1 P1 -1
         製品 名詞 一般 * * 製品 セイヒン セーヒン 0 4 C1 1",
        &[(1, -1), (0, 1)],
    )]
    fn split_prefix_accent_phrase_works(#[case] features: &str, #[case] expected: &[(i32, i32)]) {
        let actual = super::split_prefix_accent_phrase(parse_nodes(features));
        assert_eq!(expected, accents_and_chain_flags(&actual));
    }

    #[rstest]
    // 撥音上の核を1モーラ前へずらす
    #[case("缶 名詞 一般 * * 缶 カン カン 2 2 * -1", &[1])]
    // 前部末型の結合で、ア段に続く「イ」に来た核をずらす（「仙台市」）
    #[case(
        "仙台 名詞 固有名詞 * * 仙台 センダイ センダイ 4 4 C2 -1
         市 名詞 接尾 * * 市 シ シ 1 1 C3 1",
        &[3, 1],
    )]
    // 母音連続だけでは二重母音と見なさない（「津久井町」）
    #[case(
        "津久井 名詞 固有名詞 * * 津久井 ツクイ ツクイ 3 3 C2 -1
         町 名詞 接尾 * * 町 マチ マチ 1 2 C3 1",
        &[3, 1],
    )]
    // 前部末の拍に無声化記号が残る場合も1拍前へ移す（「特別市」）
    #[case(
        "特別 名詞 形容動詞語幹 * * 特別 トクベツ トクベツ’ 4 4 C2 -1
         市 名詞 接尾 * * 市 シ シ 1 1 C3 1",
        &[3, 1],
    )]
    // 2回目は動かない
    #[case(
        "特別 名詞 形容動詞語幹 * * 特別 トクベツ トクベツ’ 3 4 C2 -1
         市 名詞 接尾 * * 市 シ シ 1 1 C3 1",
        &[3, 1],
    )]
    // 拗音は1モーラとして数え、長音上の核をずらす
    #[case("東京 名詞 固有名詞 * * 東京 トウキョウ トーキョー 4 4 C2 -1", &[3])]
    fn retreat_acc_nuc_works(#[case] features: &str, #[case] expected: &[i32]) {
        let actual = super::retreat_acc_nuc(parse_nodes(features));
        assert_eq!(expected, actual.iter().map(|f| f.acc).collect::<Vec<_>>());
    }

    #[rstest]
    // 参ります → ま[いりま]す
    #[case(
        "参り 動詞 自立 五段・ラ行 連用形 参る マイリ マイリ 1 3 * -1
         ます 助動詞 * 特殊・マス 基本形 ます マス マス’ 1 2 動詞%F2@1 1",
        4
    )]
    // 未然形の「ませ」は2モーラ後ろ
    #[case(
        "参り 動詞 自立 五段・ラ行 連用形 参る マイリ マイリ 1 3 * -1
         ませ 助動詞 * 特殊・マス 未然形 ます マセ マセ 1 2 動詞%F2@1 1
         ん 助動詞 * 不変化型 基本形 ん ン ン 0 1 * 1",
        5
    )]
    // 「特殊・ナイ」は句の直前のモーラ
    #[case(
        "知ら 動詞 自立 五段・ラ行 未然形 知る シラ シラ 1 2 * -1
         ない 助動詞 * 特殊・ナイ 基本形 ない ナイ ナイ 1 2 * 1",
        2
    )]
    // 「れる」などは、その語の核を足す
    #[case(
        "書か 動詞 自立 五段・カ行イ音便 未然形 書く カカ カカ 1 2 * -1
         れる 動詞 接尾 一段 基本形 れる レル レル 1 2 * 1",
        3
    )]
    // 平板は変えない
    #[case(
        "行き 動詞 自立 五段・カ行促音便 連用形 行く イキ イキ 0 2 * -1
         ます 助動詞 * 特殊・マス 基本形 ます マス マス’ 1 2 動詞%F2@1 1",
        0
    )]
    // 核のある語の後でも、関係のない語を挟むと変えない
    #[case(
        "書い 動詞 自立 五段・カ行イ音便 連用形 書く カイ カイ 1 2 * -1
         て 助詞 接続助詞 * * て テ テ 0 1 * 1
         おり 動詞 非自立 五段・ラ行 連用形 おる オリ オリ 0 2 * 1
         ます 助動詞 * 特殊・マス 基本形 ます マス マス’ 1 2 動詞%F2@1 1",
        1
    )]
    fn modify_acc_after_chaining_works(#[case] features: &str, #[case] expected: i32) {
        let actual = super::modify_acc_after_chaining(parse_nodes(features));
        assert_eq!(expected, actual[0].acc);
    }
}
