// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/openjtalk.pyx`: `_apply_original_rule_before_chaining`)

//! アクセント句の結合（`njd_set_accent_phrase`）の前に適用する、pyopenjtalk-plus独自の規則。

use std::collections::HashSet;

use open_jtalk::NjdFeature;

use super::kana::contains_hiragana;

const DIGIT_CHARACTERS: &str = "０１２３４５６７８９〇零一二三四五六七八九";
const NUMERAL_SEPARATORS: &str = "−－ー‐‑‒–—-";

/// サ変接続・接頭語・動詞連続・連用形・助動詞などの結合規則を適用する。
///
/// `modify_numeral_reading`が`true`の場合、分数の分母の「分」を「ブン」、2つ以上続く「〇」を
/// 「マル」と読む。
pub(super) fn apply_original_rule_before_chaining(
    mut features: Vec<NjdFeature>,
    modify_numeral_reading: bool,
) -> Vec<NjdFeature> {
    let numeral_zero_indices = mark_numeral_zeros(&mut features);

    for i in 0..features.len().saturating_sub(1) {
        let (head, tail) = features.split_at_mut(i + 1);
        let (preceding, njd) = head.split_at_mut(i);
        let njd = &mut njd[0];
        let (next, following) = tail.split_first_mut().expect("`i + 1 < len`");
        let following = following.first();

        // 一般名詞の「湖」を接尾辞として読む場合、全体が4モーラ以上なら前部末型で結合する
        // 「ミズウミ」は後処理で1モーラの「コ」になるので、その長さで判定する
        if njd.pos == "名詞" && next.string == "湖" && next.pron == "ミズウミ" {
            let mut preceding_mora_size = 0;
            for f in preceding.iter().chain([&*njd]).rev() {
                if f.pos != "名詞" {
                    break;
                }
                preceding_mora_size += f.mora_size;
                if f.chain_flag == 0 {
                    break;
                }
            }
            if preceding_mora_size + 1 >= 4 {
                next.chain_rule = "C3".to_owned();
            }
        }

        // 名詞の後ろで新しい語を作る「不足」は連濁した「ブソク」と読む
        if njd.pos == "名詞" && next.string == "不足" && next.pron == "フソク" {
            next.read = "ブソク".to_owned();
            next.pron = "ブソク".to_owned();
        }

        // 分母を表す「数値 + 分 + の + 数値」だけ、時間量の「フン」・「プン」や割合の「ブ」と
        // 区別して「ブン」と読む
        let is_fraction_denominator =
            next.string == "の" && following.is_some_and(|f| f.pos_group1 == "数");
        if modify_numeral_reading && is_fraction_denominator && njd.string.ends_with('分') {
            if njd.pron.ends_with("フン") || njd.pron.ends_with("プン") {
                njd.read = replace_last_two_chars(&njd.read, "ブン");
                njd.pron = replace_last_two_chars(&njd.pron, "ブン");
            } else if njd.pron.ends_with('ブ') {
                njd.read.push('ン');
                njd.pron.push('ン');
            }
            // 後続のアクセント句が参照するモーラ数を読みに合わせる
            njd.mora_size = njd
                .pron
                .chars()
                .filter(|&c| !"ャュョァィゥェォ’".contains(c))
                .count() as _;
        }

        // 算用数字が別形態素になった分数では、数詞と「の」に挟まれた助数詞の「分」を「ブン」へ変える
        if modify_numeral_reading
            && i > 0
            && njd.string == "分"
            && preceding[i - 1].pos_group1 == "数"
            && is_fraction_denominator
        {
            njd.read = "ブン".to_owned();
            njd.pron = "ブン".to_owned();
            njd.mora_size = 2;
        }

        // 番号の桁に含まれない「〇〇町」のような伏字だけ、NJDの数字変換へ渡さずマルと読む
        // アクセント句の核はNJDの結合に任せる
        if modify_numeral_reading
            && njd.string == "〇"
            && next.string == "〇"
            && !numeral_zero_indices.contains(&i)
            && !numeral_zero_indices.contains(&(i + 1))
        {
            for placeholder in [&mut *njd, &mut *next] {
                // MeCabは後ろに何も続かない「〇〇」を記号として返すので、品詞も名詞にそろえる
                placeholder.pos = "名詞".to_owned();
                placeholder.pos_group1 = "一般".to_owned();
                placeholder.read = "マル".to_owned();
                placeholder.pron = "マル".to_owned();
                // 単独の「マル」は平板
                placeholder.acc = 0;
                placeholder.mora_size = 2;
            }
        }

        // 接尾辞「球」は漢語・外来語との生産的な結合をキュウとし、送り仮名を持つ和語だけ連濁させる
        if next.string == "球"
            && next.pos == "名詞"
            && next.pos_group1 == "接尾"
            && next.pron == "キュー"
            && njd.string.chars().any(|c| ('一'..='鿿').contains(&c))
            && contains_hiragana(&njd.string)
        {
            next.read = "ダマ".to_owned();
            next.pron = "ダマ".to_owned();
            next.acc = 1;
            next.mora_size = 2;
            next.chain_rule = "C4".to_owned();
        }

        // サ変動詞(スル)の前にサ変接続や名詞が来た場合は、一つのアクセント句に纏める
        if (["サ変接続", "格助詞", "接続助詞"].contains(&&*njd.pos_group1)
            || (njd.pos == "名詞" && njd.pos_group1 == "一般")
            || njd.pos == "副詞")
            && next.ctype == "サ変・スル"
        {
            next.chain_flag = 1;
        }

        // ご遠慮、ご配慮のような接頭語がつく場合にその後に続く単語の結合則を変更する
        if ["お", "御", "ご"].contains(&&*njd.string) && njd.chain_rule == "P1" {
            if next.acc == 0 || next.acc == next.mora_size {
                next.chain_rule = "C4".to_owned();
                next.acc = 0;
            } else {
                next.chain_rule = "C1".to_owned();
            }
        }

        // 動詞(自立)が連続する場合(ex 推し量る、刺し貫く)、後ろの動詞のアクセント核が採用される
        if njd.pos == "動詞" && next.pos == "動詞" {
            next.chain_rule = if next.acc != 0 { "C1" } else { "C4" }.to_owned();
        }

        // 連用形のアクセント核の登録を修正する
        if ["連用形", "連用タ接続", "連用ゴザイ接続", "連用テ接続"].contains(&&*njd.cform)
            && njd.acc == njd.mora_size
            && njd.mora_size > 1
        {
            njd.acc -= 1;
        }

        // 「らる、られる」＋「た」の組み合わせで「た」の助動詞/F2@0を上書きしてアクセントを
        // 下げないようにする
        if ["れる", "られる", "せる", "させる", "ちゃう"].contains(&&*njd.orig)
            && next.string == "た"
        {
            next.chain_rule = "F2@1".to_owned();
        }

        // 形容詞＋「なる、する」は一つのアクセント句に纏める
        if njd.pos == "形容詞" && ["なる", "する"].contains(&&*next.orig) {
            next.chain_flag = 1;
        }
    }

    features
}

/// 番号の桁に含まれる「〇」をゼロへ戻し、その位置を返す。
///
/// 「一〇〇一号室」の〇は番号の桁なので、伏字の「〇〇町」と区別して数詞へ渡す。
/// 「〇円」の単独の〇は数値のレーを保ち、2桁以上の番号に含まれる〇だけをゼロへ戻す。
fn mark_numeral_zeros(features: &mut [NjdFeature]) -> HashSet<usize> {
    let mut numeral_zero_indices = HashSet::new();
    let is_digit =
        |f: &NjdFeature| f.string.chars().count() == 1 && DIGIT_CHARACTERS.contains(&*f.string);
    let mut index = 0;
    while index < features.len() {
        let start = index;
        while index < features.len() && is_digit(&features[index]) {
            index += 1;
        }
        if index == start {
            index += 1;
            continue;
        }
        let mut is_number = features[start..index].iter().any(|f| f.string != "〇");
        if let Some(following) = features.get(index) {
            is_number |= following.pos_group2 == "助数詞";
            // 「〇〇〇-一二三四-五六七八」の先頭も、区切りの後に数字が続く番号として扱う
            if NUMERAL_SEPARATORS.contains(&*following.string)
                && let Some(after) = features.get(index + 1)
            {
                is_number |= DIGIT_CHARACTERS.contains(&*after.string);
            }
        }
        if is_number && index - start > 1 {
            for (zero_index, zero) in features.iter_mut().enumerate().take(index).skip(start) {
                if zero.string == "〇" {
                    numeral_zero_indices.insert(zero_index);
                    zero.pos = "名詞".to_owned();
                    zero.pos_group1 = "数".to_owned();
                    zero.read = "ゼロ".to_owned();
                    zero.pron = "ゼロ".to_owned();
                    zero.mora_size = 2;
                }
            }
        }
    }
    numeral_zero_indices
}

fn replace_last_two_chars(s: &str, replacement: &str) -> String {
    let chars = s.chars().collect::<Vec<_>>();
    let kept = chars[..chars.len().saturating_sub(2)]
        .iter()
        .collect::<String>();
    kept + replacement
}

#[cfg(test)]
mod tests {
    use open_jtalk::NjdFeature;
    use rstest::rstest;

    use super::super::test_util::node;

    fn apply(features: Vec<NjdFeature>) -> Vec<NjdFeature> {
        super::apply_original_rule_before_chaining(features, true)
    }

    fn with(mut f: NjdFeature, edit: impl FnOnce(&mut NjdFeature)) -> NjdFeature {
        edit(&mut f);
        f
    }

    fn mora(mut f: NjdFeature, acc: i32, mora_size: i32) -> NjdFeature {
        f.acc = acc;
        f.mora_size = mora_size;
        f
    }

    #[rstest]
    #[case(&[("宮沢", "ミヤザワ", 4)], "C3")]
    #[case(&[("山中", "ヤマナカ", 4)], "C3")]
    #[case(&[("津久", "ツク", 2), ("井", "イ", 1)], "C3")]
    #[case(&[("津", "ツ", 1)], "*")]
    fn lake_suffix_becomes_c3_when_long(
        #[case] preceding: &[(&str, &str, i32)],
        #[case] expected: &str,
    ) {
        let mut features = preceding
            .iter()
            .map(|&(s, p, m)| mora(node(s, "名詞", "固有名詞", p), 0, m))
            .collect::<Vec<_>>();
        features.push(mora(node("湖", "名詞", "一般", "ミズウミ"), 0, 4));
        let actual = apply(features);
        assert_eq!(expected, actual.last().unwrap().chain_rule);
    }

    #[rstest]
    fn lake_suffix_stops_counting_at_accent_phrase_boundary() {
        let features = vec![
            mora(node("長い", "名詞", "一般", "ナガイ"), 0, 3),
            with(mora(node("津", "名詞", "一般", "ツ"), 0, 1), |f| {
                f.chain_flag = 0;
            }),
            mora(node("湖", "名詞", "一般", "ミズウミ"), 0, 4),
        ];
        assert_eq!("*", apply(features)[2].chain_rule);
    }

    #[rstest]
    #[case("名詞", "ブソク")]
    #[case("助詞", "フソク")]
    fn fusoku_after_noun_becomes_busoku(#[case] preceding_pos: &str, #[case] expected: &str) {
        let features = vec![
            node("情報", preceding_pos, "一般", "ジョーホー"),
            node("不足", "名詞", "サ変接続", "フソク"),
        ];
        let actual = apply(features);
        assert_eq!(expected, actual[1].read);
        assert_eq!(expected, actual[1].pron);
    }

    #[rstest]
    #[case("三分", "サンプン", "サンブン", 4)]
    #[case("十分", "ジュップン", "ジュッブン", 4)]
    #[case("五分", "ゴフン", "ゴブン", 3)]
    #[case("一分", "イチブ", "イチブン", 4)]
    fn fraction_denominator_becomes_bun(
        #[case] string: &str,
        #[case] pron: &str,
        #[case] expected: &str,
        #[case] expected_mora_size: i32,
    ) {
        let features = vec![
            mora(node(string, "名詞", "数", pron), 0, 3),
            node("の", "助詞", "連体化", "ノ"),
            node("一", "名詞", "数", "イチ"),
        ];
        let actual = apply(features);
        assert_eq!(expected, actual[0].read);
        assert_eq!(expected, actual[0].pron);
        assert_eq!(expected_mora_size, actual[0].mora_size);
    }

    #[rstest]
    fn fraction_denominator_is_kept_without_modify_numeral_reading() {
        let features = vec![
            node("三分", "名詞", "数", "サンプン"),
            node("の", "助詞", "連体化", "ノ"),
            node("一", "名詞", "数", "イチ"),
        ];
        let actual = super::apply_original_rule_before_chaining(features, false);
        assert_eq!("サンプン", actual[0].pron);
    }

    #[rstest]
    #[case("１", "ブン")]
    // 前が数詞でなくても、「分」で終わる語として分母の規則が当たる
    #[case("猫", "ブン")]
    fn separate_fraction_counter_becomes_bun(#[case] numerator: &str, #[case] expected: &str) {
        let pos_group1 = if numerator == "１" { "数" } else { "一般" };
        let features = vec![
            node(numerator, "名詞", pos_group1, "イチ"),
            mora(node("分", "名詞", "接尾", "フン"), 1, 2),
            node("の", "助詞", "連体化", "ノ"),
            node("２", "名詞", "数", "ニ"),
        ];
        let actual = apply(features);
        assert_eq!(expected, actual[1].pron);
        assert_eq!(expected, actual[1].read);
        assert_eq!(2, actual[1].mora_size);
    }

    #[rstest]
    fn placeholder_circles_become_maru() {
        let features = vec![
            mora(node("〇", "名詞", "数", "レー"), 1, 2),
            mora(node("〇", "名詞", "数", "レー"), 1, 2),
            with(node("町", "名詞", "接尾", "マチ"), |f| {
                f.pos_group2 = "地域".to_owned()
            }),
        ];
        let actual = apply(features);
        for f in &actual[..2] {
            assert_eq!(
                ("名詞", "一般", "マル", "マル", 0, 2),
                (
                    &*f.pos,
                    &*f.pos_group1,
                    &*f.read,
                    &*f.pron,
                    f.acc,
                    f.mora_size,
                ),
            );
        }
        assert_eq!("マチ", actual[2].pron);
    }

    #[rstest]
    fn placeholder_circles_symbol_becomes_noun() {
        // 後ろに何も続かない「〇〇」はMeCabが記号として返す
        let features = vec![
            node("〇", "記号", "一般", "〇"),
            node("〇", "記号", "一般", "〇"),
        ];
        let actual = apply(features);
        assert_eq!(["名詞", "名詞"], [&*actual[0].pos, &*actual[1].pos]);
        assert_eq!(["マル", "マル"], [&*actual[0].pron, &*actual[1].pron]);
    }

    #[rstest]
    fn circles_in_number_become_zero() {
        // 「一〇〇一号室」
        let features = vec![
            mora(node("一", "名詞", "数", "イチ"), 0, 2),
            mora(node("〇", "名詞", "数", "レー"), 0, 2),
            mora(node("〇", "名詞", "数", "レー"), 0, 2),
            mora(node("一", "名詞", "数", "イチ"), 0, 2),
            with(node("号室", "名詞", "接尾", "ゴーシツ"), |f| {
                f.pos_group2 = "助数詞".to_owned();
            }),
        ];
        let actual = apply(features);
        for f in &actual[1..3] {
            assert_eq!(
                ("名詞", "数", "ゼロ", "ゼロ", 2),
                (&*f.pos, &*f.pos_group1, &*f.read, &*f.pron, f.mora_size),
            );
        }
    }

    #[rstest]
    #[case(&["〇", "〇", "〇"], &["−"], &["一"], "ゼロ")]
    #[case(&["〇", "〇"], &["助数詞"], &[], "ゼロ")]
    fn circles_followed_by_number_become_zero(
        #[case] circles: &[&str],
        #[case] following: &[&str],
        #[case] after: &[&str],
        #[case] expected: &str,
    ) {
        let mut features = circles
            .iter()
            .map(|&s| node(s, "名詞", "数", "レー"))
            .collect::<Vec<_>>();
        for &s in following {
            if s == "助数詞" {
                features.push(with(node("円", "名詞", "接尾", "エン"), |f| {
                    f.pos_group2 = "助数詞".to_owned();
                }));
            } else {
                features.push(node(s, "記号", "一般", "、"));
            }
        }
        for &s in after {
            features.push(node(s, "名詞", "数", "イチ"));
        }
        let actual = apply(features);
        assert_eq!(expected, actual[0].pron);
    }

    #[rstest]
    fn single_circle_is_kept() {
        let features = vec![
            node("〇", "名詞", "数", "レー"),
            with(node("円", "名詞", "接尾", "エン"), |f| {
                f.pos_group2 = "助数詞".to_owned();
            }),
        ];
        assert_eq!("レー", apply(features)[0].pron);
    }

    #[rstest]
    #[case("投げ", "ダマ")]
    #[case("野", "キュー")]
    #[case("なげ", "キュー")]
    fn ball_suffix_becomes_dama_after_wago(#[case] preceding: &str, #[case] expected: &str) {
        let features = vec![
            node(preceding, "名詞", "一般", "ナゲ"),
            mora(node("球", "名詞", "接尾", "キュー"), 1, 2),
        ];
        let actual = apply(features);
        assert_eq!(expected, actual[1].pron);
        if expected == "ダマ" {
            assert_eq!(
                ("ダマ", 1, 2, "C4"),
                (
                    &*actual[1].read,
                    actual[1].acc,
                    actual[1].mora_size,
                    &*actual[1].chain_rule
                )
            );
        }
    }

    #[rstest]
    #[case("名詞", "サ変接続", 1)]
    #[case("名詞", "一般", 1)]
    #[case("副詞", "一般", 1)]
    #[case("助詞", "格助詞", 1)]
    #[case("名詞", "固有名詞", -1)]
    fn suru_after_sahen_is_chained(
        #[case] pos: &str,
        #[case] pos_group1: &str,
        #[case] expected: i32,
    ) {
        let features = vec![
            node("勉強", pos, pos_group1, "ベンキョー"),
            with(node("する", "動詞", "自立", "スル"), |f| {
                f.ctype = "サ変・スル".to_owned();
            }),
        ];
        assert_eq!(expected, apply(features)[1].chain_flag);
    }

    #[rstest]
    #[case(0, 3, "C4", 0)]
    #[case(3, 3, "C4", 0)]
    #[case(1, 3, "C1", 1)]
    fn honorific_prefix_changes_chain_rule(
        #[case] acc: i32,
        #[case] mora_size: i32,
        #[case] expected_chain_rule: &str,
        #[case] expected_acc: i32,
    ) {
        let features = vec![
            with(node("ご", "接頭詞", "名詞接続", "ゴ"), |f| {
                f.chain_rule = "P1".to_owned();
            }),
            mora(node("遠慮", "名詞", "サ変接続", "エンリョ"), acc, mora_size),
        ];
        let actual = apply(features);
        assert_eq!(
            (expected_chain_rule, expected_acc),
            (&*actual[1].chain_rule, actual[1].acc),
        );
    }

    #[rstest]
    #[case(2, "C1")]
    #[case(0, "C4")]
    fn consecutive_verbs_use_latter_accent(#[case] acc: i32, #[case] expected: &str) {
        let features = vec![
            node("推し", "動詞", "自立", "オシ"),
            mora(node("量る", "動詞", "自立", "ハカル"), acc, 3),
        ];
        assert_eq!(expected, apply(features)[1].chain_rule);
    }

    #[rstest]
    #[case("連用形", 2, 2, 1)]
    #[case("連用タ接続", 3, 3, 2)]
    #[case("連用形", 1, 1, 1)]
    #[case("連用形", 1, 2, 1)]
    #[case("基本形", 2, 2, 2)]
    fn renyokei_accent_moves_forward(
        #[case] cform: &str,
        #[case] acc: i32,
        #[case] mora_size: i32,
        #[case] expected: i32,
    ) {
        let features = vec![
            with(
                mora(node("書き", "動詞", "自立", "カキ"), acc, mora_size),
                |f| {
                    f.cform = cform.to_owned();
                },
            ),
            node("ます", "助動詞", "*", "マス"),
        ];
        assert_eq!(expected, apply(features)[0].acc);
    }

    #[rstest]
    fn renyokei_rule_is_not_applied_to_last_node() {
        let features = vec![with(
            mora(node("書き", "動詞", "自立", "カキ"), 2, 2),
            |f| {
                f.cform = "連用形".to_owned();
            },
        )];
        assert_eq!(2, apply(features)[0].acc);
    }

    #[rstest]
    #[case("られる", "F2@1")]
    #[case("ちゃう", "F2@1")]
    #[case("ない", "F2@0")]
    fn passive_ta_is_not_lowered(#[case] orig: &str, #[case] expected: &str) {
        let features = vec![
            with(node("られ", "動詞", "接尾", "ラレ"), |f| {
                f.orig = orig.to_owned()
            }),
            with(node("た", "助動詞", "*", "タ"), |f| {
                f.chain_rule = "F2@0".to_owned()
            }),
        ];
        assert_eq!(expected, apply(features)[1].chain_rule);
    }

    #[rstest]
    #[case("なる", 1)]
    #[case("する", 1)]
    #[case("ある", -1)]
    fn adjective_and_naru_are_chained(#[case] orig: &str, #[case] expected: i32) {
        let features = vec![
            node("高く", "形容詞", "自立", "タカク"),
            with(node("x", "動詞", "自立", "ナル"), |f| {
                f.orig = orig.to_owned()
            }),
        ];
        assert_eq!(expected, apply(features)[1].chain_flag);
    }

    #[rstest]
    fn empty_is_ok() {
        assert_eq!(Vec::<NjdFeature>::new(), apply(vec![]));
    }
}
