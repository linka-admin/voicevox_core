// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)
// (`pyopenjtalk/openjtalk.pyx`: `_apply_original_rule_before_chaining`)

//! アクセント句の結合（`njd_set_accent_phrase`）の前に適用する、pyopenjtalk-plus独自の規則。

use std::collections::HashSet;

use open_jtalk::NjdFeature;

use super::{accent::split_prefix_accent_phrase, kana::contains_hiragana};

const DIGIT_CHARACTERS: &str = "０１２３４５６７８９〇零一二三四五六七八九";
const NUMERAL_SEPARATORS: &str = "−－ー‐‑‒–—-";

/// 数詞の直後で英字の記号として解析された1文字の単位を、助数詞として読むための読み・アクセント核・
/// モーラ数・結合規則。
///
/// 値は既定辞書の助数詞の行と同じにして、「四千ｇ」と辞書の助数詞が選ばれた場合とで読みとアクセントを
/// そろえる。1文字の単位の助数詞の行はコストが高く、「四千ｇだった」「八十ｍ先」では英字の記号に負けて
/// 単位が読まれない。辞書のコストを下げると読点の後の「ｔ値」や数式の変数の「ｍ」まで助数詞になるので、
/// 数詞の直後に限って読み替える。
const UNIT_LETTER_COUNTERS: &[(&str, &str, i32, i32, &str)] = &[
    ("ｇ", "グラム", 1, 3, "C1"),
    ("ｍ", "メートル", 0, 4, "C2"),
    ("ｔ", "トン", 1, 2, "C3"),
    ("ｌ", "リットル", 0, 4, "C2"),
];

/// 単位の英字の直後にあると、その英字を数式の変数とみなす演算子（「2m+1」の「＋」など）。
///
/// 「1m60cm」の数字、「五百ｇ／タマネギ」の区切りの斜線、空白を挟んだ「七十ｇ　Ａ」の英字は単位の後にも
/// 続くので含めない。
const UNIT_LETTER_FORMULA_OPERATORS: &str = "＋−－＝×÷＊＜＞+-=*<>";

/// サ変接続・接頭語・動詞連続・連用形・助動詞などの結合規則を適用する。
///
/// `modify_numeral_reading`が`true`の場合、分数の分母の「分」を「ブン」、2つ以上続く「〇」を
/// 「マル」と読み、数詞の直後の「ｇ」「ｍ」などの単位の英字を助数詞として読む。また、接頭辞の後ろの
/// アクセント句を分ける（[`split_prefix_accent_phrase`]）。
pub(super) fn apply_original_rule_before_chaining(
    mut features: Vec<NjdFeature>,
    modify_numeral_reading: bool,
) -> Vec<NjdFeature> {
    let numeral_zero_indices = mark_numeral_zeros(&mut features);
    let len = features.len();

    for i in 0..len.saturating_sub(1) {
        // ナイ形に続く「の」「と」は、助動詞に付く結合規則を明示して否定形の核を保つ
        // 助動詞用の規則がないと結合規則の末尾のF2が適用され、平板型の「知らないのか」が
        // 「シラナイ＼ノカ」になる
        if features[i].pos == "助動詞"
            && features[i].orig == "ない"
            && (features[i + 1].pos == "助詞"
                || (features[i + 1].pos == "名詞" && features[i + 1].pos_group1 == "非自立"))
            && ["の", "と"].contains(&&*features[i + 1].orig)
        {
            let next = &mut features[i + 1];
            next.chain_rule = std::iter::once("助動詞%F1")
                .chain(
                    next.chain_rule
                        .split('/')
                        .filter(|rule| !rule.starts_with("助動詞%")),
                )
                .collect::<Vec<_>>()
                .join("/");
        }

        // 放置を意味する「放っておく」「放っとく」の「放っ」は、読み補正で平板型の「放る」の
        // 「ホーッ」に替えるため、結合の前に核を0にする
        // 結合した後で核だけを書き換えると、「放っておきません」「放っておきなさい」で後続の語が
        // 置く核まで消えてしまう
        if features[i].string == "放っ"
            && ((features[i + 1].string == "て"
                && features
                    .get(i + 2)
                    .is_some_and(|f| f.orig == "おく" && f.pos == "動詞"))
                || (features[i + 1].orig == "とく" && features[i + 1].pos == "動詞"))
        {
            features[i].acc = 0;
        }

        // 動詞の意向形の「う」は、付属語の「う」が核を決める型で、「守ろう」「盛ろう」を
        // 「マモロ＼ー」「モロ＼ー」と「う」の直前の拍の後で下げる
        // 推量の「だろう」の「う」は直前が助動詞なので変えない
        // 平板の動詞の意向形に「と」と「する」「思う」が続く「盛ろうとする」は、平板のまま読む
        if features[i].pos == "動詞"
            && features[i].cform == "未然ウ接続"
            && features[i + 1].pos == "助動詞"
            && features[i + 1].orig == "う"
        {
            let is_flat_before_to_suru = features[i].acc == 0
                && i + 3 < len
                && features[i + 2].string == "と"
                && ["する", "思う"].contains(&&*features[i + 3].orig);
            if !is_flat_before_to_suru {
                features[i + 1].chain_rule = "動詞%F4@0/特殊助動詞%F2@0".to_owned();
            }
        }

        // 平板の動詞に使役の「せ」「させ」と「ない」が続く「聞かせない」「歌わせない」は、平板の
        // 「キカセナイ」「ウタワセナイ」と読む
        // 「せ」の核をなくすと、後ろの動詞の連続の規則が平板のC4で結合し、「ない」は直前の動詞の型を
        // 保つF1で続く。起伏の動詞の「書かせない」は、今までどおり「カカセ＼ナイ」と読む
        let is_causative_after_flat_verb = features[i].pos == "動詞"
            && features[i].pos_group1 == "自立"
            && features[i].acc == 0
            && features[i + 1].pos == "動詞"
            && features[i + 1].pos_group1 == "接尾"
            && ["せる", "させる"].contains(&&*features[i + 1].orig);
        if is_causative_after_flat_verb
            && features
                .get(i + 2)
                .is_some_and(|f| f.pos == "助動詞" && f.orig == "ない")
        {
            features[i + 1].acc = 0;
            let nai = &mut features[i + 2];
            nai.chain_rule = nai
                .chain_rule
                .split('/')
                .map(|rule| {
                    if rule.starts_with("動詞%") {
                        "動詞%F1"
                    } else {
                        rule
                    }
                })
                .collect::<Vec<_>>()
                .join("/");
        }
        // 平板の動詞の使役の基本形「聞かせる」「歌わせる」も、平板の「キカセル」「ウタワセル」と読む
        // 起伏の動詞の「書かせる」は「カカセ＼ル」のまま読み、「聞かせます」「聞かせれば」は付属語の
        // 規則で核を置く
        if is_causative_after_flat_verb && features[i + 1].cform == "基本形" {
            features[i + 1].acc = 0;
        }

        // 一般名詞の「湖」を接尾辞として読む場合、全体が4モーラ以上なら前部末型で結合する
        // 「ミズウミ」は後処理で1モーラの「コ」になるので、その長さで判定する
        if features[i].pos == "名詞"
            && features[i + 1].string == "湖"
            && features[i + 1].pron == "ミズウミ"
        {
            let mut preceding_mora_size = 0;
            for f in features[..=i].iter().rev() {
                if f.pos != "名詞" {
                    break;
                }
                preceding_mora_size += f.mora_size;
                if f.chain_flag == 0 {
                    break;
                }
            }
            if preceding_mora_size + 1 >= 4 {
                features[i + 1].chain_rule = "C3".to_owned();
            }
        }

        // 名詞の後ろで新しい語を作る「不足」は連濁した「ブソク」と読む
        if features[i].pos == "名詞"
            && features[i + 1].string == "不足"
            && features[i + 1].pron == "フソク"
        {
            features[i + 1].read = "ブソク".to_owned();
            features[i + 1].pron = "ブソク".to_owned();
        }

        // 分母を表す「数値 + 分 + の + 数値」だけ、時間量の「フン」・「プン」や割合の「ブ」と
        // 区別して「ブン」と読む
        let mut is_fraction_denominator = false;
        if i + 2 < len && features[i + 1].string == "の" {
            // 「3分の何」「何分の何」の分子の「何」は数を問う語なので、数詞の「ナン」と読み、分母を
            // 「ブン」にする
            // 「5分の何もない時間」のように「も」が続く「何」は、数でない「ナニ」の用法なので除く
            // 「千分の何がし」はMeCabが「何」「が」「し」に分けるが、数でない「ナニガシ」なので除く
            if modify_numeral_reading && is_fraction_numerator_nani(&features, i) {
                let following = &mut features[i + 2];
                following.pos = "名詞".to_owned();
                following.pos_group1 = "数".to_owned();
                following.pos_group2 = "*".to_owned();
                following.pos_group3 = "*".to_owned();
                following.read = "ナン".to_owned();
                following.pron = "ナン".to_owned();
                following.acc = 1;
                following.mora_size = 2;
                following.chain_rule = "C3".to_owned();
            }
            is_fraction_denominator = features[i + 2].pos_group1 == "数";
        }
        if modify_numeral_reading && is_fraction_denominator && features[i].string.ends_with('分')
        {
            let is_counter_after_numeral =
                features[i].string == "分" && i > 0 && features[i - 1].pos_group1 == "数";
            let njd = &mut features[i];
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
            // 分数は「万分の一」（「マンブンノイチ＼」）と同じく、分母を平板にして分子の数詞の核で
            // 1つのアクセント句に読む
            // 数詞に続く助数詞の「分」は、数詞との結合後の句を平板にするF5にし、「三分」のような
            // 1語の分母は核をなくす
            if is_counter_after_numeral {
                njd.chain_rule = "F5".to_owned();
            } else {
                njd.acc = 0;
            }
            features[i + 1].chain_flag = 1;
            features[i + 2].chain_flag = 1;
            features[i + 2].chain_rule = "C1".to_owned();
        }

        // 算用数字が別形態素になった分数では、数詞と「の」に挟まれた助数詞の「分」を「ブン」へ変える
        if modify_numeral_reading
            && i > 0
            && features[i].string == "分"
            && features[i - 1].pos_group1 == "数"
            && is_fraction_denominator
        {
            let njd = &mut features[i];
            njd.read = "ブン".to_owned();
            njd.pron = "ブン".to_owned();
            njd.mora_size = 2;
        }

        // 数詞の直後で英字の記号として解析された「ｇ」「ｍ」などの単位を、辞書の助数詞と同じ読みと
        // アクセントに置き換える
        // 助数詞に変えておくと、後段のNJDの数詞処理が「サンゼングラム」のように数詞と1つの
        // アクセント句にまとめる。「2m+1」「x＝2ｔ×3」のように英字の直後に演算子が続く場合は、
        // 数式の変数として英字のまま読む
        if modify_numeral_reading
            && features[i].pos_group1 == "数"
            && features[i + 1].pos == "記号"
            && !features
                .get(i + 2)
                .and_then(|f| f.string.chars().next())
                .is_some_and(|c| UNIT_LETTER_FORMULA_OPERATORS.contains(c))
            && let Some(&(_, read, acc, mora_size, chain_rule)) = UNIT_LETTER_COUNTERS
                .iter()
                .find(|(letter, ..)| *letter == features[i + 1].string)
        {
            let unit = &mut features[i + 1];
            unit.pos = "名詞".to_owned();
            unit.pos_group1 = "接尾".to_owned();
            unit.pos_group2 = "助数詞".to_owned();
            unit.pos_group3 = "*".to_owned();
            unit.read = read.to_owned();
            unit.pron = read.to_owned();
            unit.acc = acc;
            unit.mora_size = mora_size;
            unit.chain_rule = chain_rule.to_owned();
        }

        // 番号の桁に含まれない「〇〇町」のような伏字だけ、NJDの数字変換へ渡さずマルと読む
        // アクセント句の核はNJDの結合に任せる
        if modify_numeral_reading
            && features[i].string == "〇"
            && features[i + 1].string == "〇"
            && !numeral_zero_indices.contains(&i)
            && !numeral_zero_indices.contains(&(i + 1))
        {
            for placeholder in &mut features[i..=i + 1] {
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

        let (head, tail) = features.split_at_mut(i + 1);
        let njd = &mut head[i];
        let next = &mut tail[0];

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

    // フィラーは前の語の句につなげず、フィラー自身と直後の名詞からそれぞれ新しいアクセント句を始める
    // 句の切れ目をNJDが核を計算した後で変えると、前の句の核が句の外に残る（「東京タワーえーと
    // スカイツリー」が「タワー」の後で下がらない）ので、核の計算の前に決める
    for i in 0..len {
        if features[i].pos != "フィラー" {
            continue;
        }
        features[i].chain_flag = 0;
        if let Some(next) = features.get_mut(i + 1)
            && next.pos == "名詞"
        {
            next.chain_flag = 0;
        }
    }

    if modify_numeral_reading {
        // 「当大学病院」「本研究計画」は、接頭辞の後ろで句を分けてから複合語の核を計算する
        // 計算後に句だけを分けると、接頭辞につながった間に失われた後ろの句の核を復元できない
        features = split_prefix_accent_phrase(features);
    }

    features
}

/// 「3分の何」「何分の何」のように、分母の「分」の後ろの「の」に続く、数を問う「何」か。
fn is_fraction_numerator_nani(features: &[NjdFeature], i: usize) -> bool {
    let njd = &features[i];
    let Some(denominator) = njd.string.strip_suffix('分') else {
        return false;
    };
    let following = &features[i + 2];
    ((!denominator.is_empty()
        && denominator
            .chars()
            .all(|c| DIGIT_CHARACTERS.contains(c) || "十百千万何".contains(c)))
        || (i > 0 && features[i - 1].pos_group1 == "数"))
        && following.string == "何"
        && following.pos_group1 != "数"
        && features.get(i + 3).is_none_or(|f| f.string != "も")
        && !(i + 4 < features.len()
            && features[i + 3].string == "が"
            && features[i + 4].string == "し")
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
        // 「〇一二〇-九九九-〇〇〇」の最後の組と、「一,〇〇〇」の3桁区切りの後の組も、数字の後の
        // 区切りに続く数として扱う
        if start >= 2
            && "−－ー‐‑‒–—-，".contains(&*features[start - 1].string)
            && DIGIT_CHARACTERS.contains(&*features[start - 2].string)
        {
            is_number = true;
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
    #[case("助詞", "*", "の", "*", "助動詞%F1/*")]
    #[case("名詞", "非自立", "の", "動詞%F2@1/助動詞%F2@0", "助動詞%F1/動詞%F2@1")]
    #[case("助詞", "*", "と", "形容詞%F1", "助動詞%F1/形容詞%F1")]
    #[case("助詞", "*", "が", "形容詞%F1", "形容詞%F1")]
    fn particle_after_nai_keeps_negative_nucleus(
        #[case] pos: &str,
        #[case] pos_group1: &str,
        #[case] orig: &str,
        #[case] chain_rule: &str,
        #[case] expected: &str,
    ) {
        let features = vec![
            node("ない", "助動詞", "*", "ナイ"),
            with(node(orig, pos, pos_group1, "ノ"), |f| {
                f.chain_rule = chain_rule.to_owned();
            }),
        ];
        assert_eq!(expected, apply(features)[1].chain_rule);
    }

    #[rstest]
    #[case(&[("放っ", "動詞"), ("て", "助詞"), ("おく", "動詞")], 0)]
    #[case(&[("放っ", "動詞"), ("とく", "動詞")], 0)]
    #[case(&[("放っ", "動詞"), ("た", "助動詞")], 2)]
    fn houtte_oku_becomes_heiban(#[case] words: &[(&str, &str)], #[case] expected: i32) {
        let features = words
            .iter()
            .map(|&(s, pos)| mora(node(s, pos, "自立", "ホーッ"), 2, 3))
            .collect::<Vec<_>>();
        assert_eq!(expected, apply(features)[0].acc);
    }

    #[rstest]
    // 守ろう
    #[case(2, &[], "動詞%F4@0/特殊助動詞%F2@0")]
    // 盛ろうとする: 平板の動詞に「と」「する」が続く
    #[case(0, &[("と", "助詞"), ("する", "動詞")], "*")]
    // 守ろうとする: 起伏の動詞は「と」「する」が続いても変える
    #[case(2, &[("と", "助詞"), ("する", "動詞")], "動詞%F4@0/特殊助動詞%F2@0")]
    fn volitional_u_lowers_after_preceding_mora(
        #[case] acc: i32,
        #[case] following: &[(&str, &str)],
        #[case] expected: &str,
    ) {
        let mut features = vec![
            with(
                mora(node("守ろ", "動詞", "自立", "マモロ"), acc, 3),
                |f| {
                    f.cform = "未然ウ接続".to_owned();
                },
            ),
            with(node("う", "助動詞", "*", "ー"), |f| {
                f.orig = "う".to_owned();
            }),
        ];
        features.extend(following.iter().map(|&(s, pos)| node(s, pos, "*", "ト")));
        assert_eq!(expected, apply(features)[1].chain_rule);
    }

    #[rstest]
    // 聞かせない
    #[case(0, "ない", "*", 0, "動詞%F1/形容詞%F2")]
    // 書かせない: 起伏の動詞は変えない
    #[case(2, "ない", "*", 1, "動詞%F2@0/形容詞%F2")]
    // 聞かせる
    #[case(0, "", "基本形", 0, "")]
    // 聞かせます: 付属語の規則に任せる
    #[case(0, "ます", "連用形", 1, "動詞%F2@0/形容詞%F2")]
    fn causative_after_heiban_verb_becomes_heiban(
        #[case] verb_acc: i32,
        #[case] auxiliary: &str,
        #[case] cform: &str,
        #[case] expected_acc: i32,
        #[case] expected_chain_rule: &str,
    ) {
        let mut features = vec![
            mora(node("聞か", "動詞", "自立", "キカ"), verb_acc, 2),
            with(mora(node("せ", "動詞", "接尾", "セ"), 1, 1), |f| {
                f.orig = "せる".to_owned();
                f.cform = cform.to_owned();
            }),
        ];
        if !auxiliary.is_empty() {
            features.push(with(node(auxiliary, "助動詞", "*", "ナイ"), |f| {
                f.chain_rule = "動詞%F2@0/形容詞%F2".to_owned();
            }));
        }
        let actual = apply(features);
        assert_eq!(expected_acc, actual[1].acc);
        if !auxiliary.is_empty() {
            assert_eq!(expected_chain_rule, actual[2].chain_rule);
        }
    }

    #[rstest]
    // 三分の何
    #[case(&["三分", "の", "何"], true)]
    #[case(&["何分", "の", "何"], true)]
    // 五分の何もない時間
    #[case(&["五分", "の", "何", "も"], false)]
    // 千分の何がし
    #[case(&["千分", "の", "何", "が", "し"], false)]
    fn fraction_numerator_nani_becomes_nan(#[case] strings: &[&str], #[case] expected: bool) {
        let features = strings
            .iter()
            .map(|&s| match s {
                "の" => node(s, "助詞", "連体化", "ノ"),
                "何" => node(s, "名詞", "代名詞", "ナニ"),
                s if s.ends_with('分') => mora(node(s, "名詞", "数", "サンプン"), 0, 4),
                s => node(s, "助詞", "*", s),
            })
            .collect::<Vec<_>>();
        let actual = apply(features);
        assert_eq!(expected, actual[2].pron == "ナン");
        if expected {
            assert_eq!(
                // 分子の「何」の結合規則は、分数を1つの句にする規則で`C1`になる
                ("数", 1, 2, "C1"),
                (
                    &*actual[2].pos_group1,
                    actual[2].acc,
                    actual[2].mora_size,
                    &*actual[2].chain_rule
                ),
            );
            assert_eq!("サンブン", actual[0].pron);
        }
    }

    #[rstest]
    fn fraction_is_one_accent_phrase() {
        // 三分の一: 1語の分母は核をなくし、分子までつなげる
        let features = vec![
            mora(node("三分", "名詞", "数", "サンプン"), 3, 4),
            node("の", "助詞", "連体化", "ノ"),
            mora(node("一", "名詞", "数", "イチ"), 2, 2),
        ];
        let actual = apply(features);
        assert_eq!(
            [(0, -1, "*"), (0, 1, "*"), (2, 1, "C1")],
            [0, 1, 2].map(|i| (actual[i].acc, actual[i].chain_flag, &*actual[i].chain_rule)),
        );
    }

    #[rstest]
    fn separate_fraction_counter_becomes_f5() {
        // 3分の1
        let features = vec![
            node("３", "名詞", "数", "サン"),
            mora(node("分", "名詞", "接尾", "フン"), 1, 2),
            node("の", "助詞", "連体化", "ノ"),
            node("１", "名詞", "数", "イチ"),
        ];
        let actual = apply(features);
        assert_eq!(("F5", 1), (&*actual[1].chain_rule, actual[1].acc));
        assert_eq!((1, "C1"), (actual[3].chain_flag, &*actual[3].chain_rule));
    }

    #[rstest]
    #[case("ｇ", "", Some(("グラム", 1, 3, "C1")))]
    #[case("ｍ", "先", Some(("メートル", 0, 4, "C2")))]
    #[case("ｔ", "", Some(("トン", 1, 2, "C3")))]
    #[case("ｌ", "", Some(("リットル", 0, 4, "C2")))]
    #[case("ｍ", "＋", None)]
    #[case("ｍ", "+1", None)]
    #[case("ｋ", "", None)]
    fn unit_letter_after_numeral_becomes_counter(
        #[case] letter: &str,
        #[case] following: &str,
        #[case] expected: Option<(&str, i32, i32, &str)>,
    ) {
        let mut features = vec![
            node("四千", "名詞", "数", "ヨンセン"),
            node(letter, "記号", "アルファベット", letter),
        ];
        if !following.is_empty() {
            features.push(node(following, "名詞", "一般", "サキ"));
        }
        let actual = apply(features);
        let unit = &actual[1];
        match expected {
            Some((read, acc, mora_size, chain_rule)) => assert_eq!(
                (
                    "名詞",
                    "接尾",
                    "助数詞",
                    read,
                    read,
                    acc,
                    mora_size,
                    chain_rule
                ),
                (
                    &*unit.pos,
                    &*unit.pos_group1,
                    &*unit.pos_group2,
                    &*unit.read,
                    &*unit.pron,
                    unit.acc,
                    unit.mora_size,
                    &*unit.chain_rule,
                ),
            ),
            None => assert_eq!("記号", unit.pos),
        }
    }

    #[rstest]
    // 「〇一二〇-九九九-〇〇〇」の最後の組
    #[case(&["九", "−", "〇", "〇", "〇"], "ゼロ")]
    // 「一，〇〇〇」
    #[case(&["一", "，", "〇", "〇", "〇"], "ゼロ")]
    #[case(&["町", "−", "〇", "〇"], "マル")]
    fn circles_after_number_separator_become_zero(
        #[case] strings: &[&str],
        #[case] expected: &str,
    ) {
        let features = strings
            .iter()
            .map(|&s| match s {
                "−" | "，" => node(s, "記号", "一般", "、"),
                "町" => node(s, "名詞", "一般", "マチ"),
                s => node(s, "名詞", "数", "レー"),
            })
            .collect::<Vec<_>>();
        assert_eq!(expected, apply(features)[2].pron);
    }

    #[rstest]
    fn filler_starts_new_accent_phrases() {
        // 東京タワーえーとスカイツリー
        let features = vec![
            node("タワー", "名詞", "一般", "タワー"),
            node("えーと", "フィラー", "*", "エート"),
            node("スカイツリー", "名詞", "固有名詞", "スカイツリー"),
            node("だ", "助動詞", "*", "ダ"),
        ];
        let actual = apply(features);
        assert_eq!([-1, 0, 0, -1], [0, 1, 2, 3].map(|i| actual[i].chain_flag),);
    }

    #[rstest]
    fn independent_prefix_splits_before_chaining() {
        // 当大学病院
        let features = vec![
            node("当", "接頭詞", "名詞接続", "トー"),
            node("大学", "名詞", "一般", "ダイガク"),
            node("病院", "名詞", "一般", "ビョーイン"),
        ];
        let actual = apply(features);
        assert_eq!([-1, 0, -1], [0, 1, 2].map(|i| actual[i].chain_flag));
        let actual = super::apply_original_rule_before_chaining(
            vec![
                node("当", "接頭詞", "名詞接続", "トー"),
                node("大学", "名詞", "一般", "ダイガク"),
            ],
            false,
        );
        assert_eq!(-1, actual[1].chain_flag);
    }

    #[rstest]
    fn empty_is_ok() {
        assert_eq!(Vec::<NjdFeature>::new(), apply(vec![]));
    }
}
