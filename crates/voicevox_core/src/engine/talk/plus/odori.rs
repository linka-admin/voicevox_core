// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/utils.py`: `detect_odori_unit`, `process_odori_features`)

//! 踊り字（々）と一の字点（ゝ、ゞ、ヽ、ヾ）の読みの後処理。

use open_jtalk::NjdFeature;

use super::kana::{SMALL_KANA, split_kana_mora};

/// 一度に遡って集める漢字の最大の文字数。
const MAX_ODORI_BASE_CHARS: usize = 8;

/// 濁音と、対応する清音（`_SEION_CHAR_MAP`）。
const SEION_CHARS: &[(char, char)] = &[
    ('が', 'か'),
    ('ぎ', 'き'),
    ('ぐ', 'く'),
    ('げ', 'け'),
    ('ご', 'こ'),
    ('ざ', 'さ'),
    ('じ', 'し'),
    ('ず', 'す'),
    ('ぜ', 'せ'),
    ('ぞ', 'そ'),
    ('だ', 'た'),
    ('ぢ', 'ち'),
    ('づ', 'つ'),
    ('で', 'て'),
    ('ど', 'と'),
    ('ば', 'は'),
    ('び', 'ひ'),
    ('ぶ', 'ふ'),
    ('べ', 'へ'),
    ('ぼ', 'ほ'),
    ('ガ', 'カ'),
    ('ギ', 'キ'),
    ('グ', 'ク'),
    ('ゲ', 'ケ'),
    ('ゴ', 'コ'),
    ('ザ', 'サ'),
    ('ジ', 'シ'),
    ('ズ', 'ス'),
    ('ゼ', 'セ'),
    ('ゾ', 'ソ'),
    ('ダ', 'タ'),
    ('ヂ', 'チ'),
    ('ヅ', 'ツ'),
    ('デ', 'テ'),
    ('ド', 'ト'),
    ('バ', 'ハ'),
    ('ビ', 'ヒ'),
    ('ブ', 'フ'),
    ('ベ', 'ヘ'),
    ('ボ', 'ホ'),
    ('ヴ', 'ウ'),
];

/// 一の字点の濁点化に使う、清音と濁音のモーラ（`process_odoriji`の`dakuten_map`）。
const DAKUTEN_MORAS: &[(&str, &str)] = &[
    ("カ", "ガ"),
    ("キ", "ギ"),
    ("ク", "グ"),
    ("ケ", "ゲ"),
    ("コ", "ゴ"),
    ("サ", "ザ"),
    ("シ", "ジ"),
    ("ス", "ズ"),
    ("セ", "ゼ"),
    ("ソ", "ゾ"),
    ("タ", "ダ"),
    ("チ", "ヂ"),
    ("ツ", "ヅ"),
    ("テ", "デ"),
    ("ト", "ド"),
    ("ハ", "バ"),
    ("ヒ", "ビ"),
    ("フ", "ブ"),
    ("ヘ", "ベ"),
    ("ホ", "ボ"),
    ("か", "が"),
    ("き", "ぎ"),
    ("く", "ぐ"),
    ("け", "げ"),
    ("こ", "ご"),
    ("さ", "ざ"),
    ("し", "じ"),
    ("す", "ず"),
    ("せ", "ぜ"),
    ("そ", "ぞ"),
    ("た", "だ"),
    ("ち", "ぢ"),
    ("つ", "づ"),
    ("て", "で"),
    ("と", "ど"),
    ("は", "ば"),
    ("ひ", "び"),
    ("ふ", "ぶ"),
    ("へ", "べ"),
    ("ほ", "ぼ"),
    ("キャ", "ギャ"),
    ("キュ", "ギュ"),
    ("キョ", "ギョ"),
    ("シャ", "ジャ"),
    ("シュ", "ジュ"),
    ("ショ", "ジョ"),
    ("チャ", "ヂャ"),
    ("チュ", "ヂュ"),
    ("チョ", "ヂョ"),
    ("ヒャ", "ビャ"),
    ("ヒュ", "ビュ"),
    ("ヒョ", "ビョ"),
    ("きゃ", "ぎゃ"),
    ("きゅ", "ぎゅ"),
    ("きょ", "ぎょ"),
    ("しゃ", "じゃ"),
    ("しゅ", "じゅ"),
    ("しょ", "じょ"),
    ("ちゃ", "ぢゃ"),
    ("ちゅ", "ぢゅ"),
    ("ちょ", "ぢょ"),
    ("ひゃ", "びゃ"),
    ("ひゅ", "びゅ"),
    ("ひょ", "びょ"),
];

/// 繰り返しの単位を清音化した読みの末尾から探す。
///
/// 「々」の展開で直前の語が既に踊り字を展開したものの場合に、繰り返しの単位を特定するために使う。
///
/// 例: 「サマザマ」→ 清音化 →「サマサマ」→ 周期2（「サマ」の繰り返し）
pub(super) fn detect_odori_unit(read: &str) -> Option<usize> {
    let seion_read = read
        .chars()
        .map(|c| {
            SEION_CHARS
                .iter()
                .find(|&&(voiced, _)| voiced == c)
                .map_or(c, |&(_, voiceless)| voiceless)
        })
        .collect::<String>();
    let moras = split_kana_mora(&seion_read);
    let n = moras.len();
    // 後ろ半分が前半分と一致する最小の単位を探す
    (1..=n / 2).find(|&period| moras[n - period * 2..n - period] == moras[n - period..])
}

/// 踊り字と一の字点を、直前の語の読みで展開する。
///
/// 単独の踊り字の直前が複数の漢字からなる語の場合は、その最後の漢字（と直後の1文字の漢字）を
/// `reanalyze`で解析し直す。`reanalyze`はpyopenjtalk-plusの`OpenJTalk.run_frontend`に相当する。
///
/// - 「叙々々苑」→「ジョジョジョエン」
/// - 「部分々々」→「ブブンブブン」
/// - 「民主々義」→「ミンシュシュギ」
/// - 「みすゞ」→「ミスズ」
pub(super) fn process_odori_features(
    mut features: Vec<NjdFeature>,
    reanalyze: &mut dyn FnMut(&str) -> anyhow::Result<Vec<NjdFeature>>,
) -> anyhow::Result<Vec<NjdFeature>> {
    let mut i = 0;
    while i < features.len() {
        if is_dancing(&features[i].orig) {
            // 単独の踊り字で再解析が必要な場合
            if i > 0
                && let Some((target_kanji, next_kanji)) =
                    needs_reanalysis(&features[i], &features[i - 1], features.get(i + 1))
            {
                if let Some(next_kanji) = next_kanji {
                    // 再解析結果は直前の語の一部を繰り返して合成した語なので、直前の語に連結させる
                    let mut analyzed = reanalyze(&format!("{target_kanji}{next_kanji}"))?;
                    if let Some(first) = analyzed.first_mut() {
                        first.chain_flag = 1;
                    }
                    // 再解析結果を踊り字に反映し、後続の漢字を削除する
                    let len = analyzed.len();
                    features.splice(i..i + 2, analyzed);
                    i += len;
                } else {
                    // 最後の漢字のみを再解析する
                    if let Some(mut analyzed) =
                        reanalyze(&target_kanji.to_string())?.into_iter().next()
                    {
                        // 踊り字は直前の語の繰り返しなので連結させる
                        analyzed.chain_flag = 1;
                        make_common_noun(&mut analyzed);
                        features[i] = analyzed;
                    }
                    i += 1;
                }
                continue;
            }

            // 連続する踊り字を特定する
            let start = i;
            let end = start
                + features[start..]
                    .iter()
                    .take_while(|f| is_dancing(&f.orig))
                    .count();
            let total_odori = features[start..end]
                .iter()
                .map(|f| count_odori(&f.orig))
                .sum::<usize>();

            // 直前の語が「々」で終わる（既に踊り字を展開した）場合は、清音化した読みで繰り返しの単位を
            // 探して展開する
            if i > 0 && features[i - 1].orig.ends_with('々') {
                let previous = &features[i - 1];
                if let Some(period) = detect_odori_unit(&previous.read) {
                    let read_moras = split_kana_mora(&previous.read);
                    let pron_moras = split_kana_mora(&previous.pron);
                    let unit_read = read_moras[read_moras.len() - period..].concat();
                    let unit_pron = pron_moras[py_slice_start(pron_moras.len(), period)..].concat();
                    let unit_mora =
                        previous.mora_size.div_euclid(read_moras.len() as i32) * period as i32;
                    let base_acc = previous.acc;

                    let current = &mut features[i];
                    let current_odori = count_odori(&current.orig);
                    current.read = unit_read.repeat(current_odori);
                    current.pron = unit_pron.repeat(current_odori);
                    current.mora_size = unit_mora * current_odori as i32;
                    current.acc = base_acc;
                    current.chain_flag = 1;
                    make_common_noun_if_symbol(current);
                    i += 1;
                    continue;
                }
            }

            // 直前の漢字の語を遡って集める
            // 記号・フィラー・感動詞を境界として、遠くの無関係な語を参照しないようにする
            let needed_chars = total_odori.min(MAX_ODORI_BASE_CHARS);
            let mut base_start = start;
            let mut collected_chars = 0;
            for target in features[..start].iter().rev() {
                if ["記号", "フィラー", "感動詞"].contains(&&*target.pos) || !is_kanji_token(target)
                {
                    break;
                }
                base_start -= 1;
                collected_chars += target.orig.chars().count();
                if collected_chars >= needed_chars {
                    break;
                }
            }
            let base = &features[base_start..start];

            // 前に適切な漢字がない場合は展開しない
            let Some(first_base) = base.first() else {
                i = end;
                continue;
            };

            // 1文字の漢字は踊り字の数だけ繰り返し、複数の漢字はそのまま使う
            let is_single_kanji = base.len() == 1 && first_base.orig.chars().count() == 1;
            let base_read = base.iter().map(|f| &*f.read).collect::<String>();
            let base_pron = base.iter().map(|f| &*f.pron).collect::<String>();
            let base_mora_size = base.iter().map(|f| f.mora_size).sum::<i32>();
            // 直前の語のアクセント核を踊り字の読みに引き継ぐ
            let base_acc = first_base.acc;

            for feature in &mut features[start..end] {
                let repeat = if is_single_kanji {
                    count_odori(&feature.orig)
                } else {
                    1
                };
                feature.read = base_read.repeat(repeat);
                feature.pron = base_pron.repeat(repeat);
                feature.mora_size = base_mora_size * repeat as i32;
                // 踊り字は直前の語の繰り返しなので連結させる
                feature.acc = base_acc;
                feature.chain_flag = 1;
                make_common_noun_if_symbol(feature);
            }
            i = end;
        } else if is_odoriji(&features[i].orig) {
            // 直前が記号の場合は、絵文字や装飾的なものとみなして展開しない
            if i > 0 && features[i - 1].pos != "記号" {
                // 記号でなく、モーラを持つ直前の語を基準にする
                if let Some(previous) = features[..i]
                    .iter()
                    .rposition(|f| f.pos != "記号" && f.mora_size > 0)
                {
                    let previous = features[previous].clone();
                    process_odoriji(&mut features[i], &previous);
                }
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    Ok(features)
}

/// 踊り字「々」だけからなるか。
fn is_dancing(orig: &str) -> bool {
    !orig.is_empty() && orig.chars().all(|c| c == '々')
}

/// 一の字点（ゝ、ゞ、ヽ、ヾ）だけからなるか（空文字列を含む）。
fn is_odoriji(orig: &str) -> bool {
    orig.chars().all(|c| "ゝゞヽヾ".contains(c))
}

fn count_odori(orig: &str) -> usize {
    orig.chars().filter(|&c| c == '々').count()
}

fn is_cjk_unified_ideograph(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
}

/// 記号でなく、原形に漢字を含むか。
fn is_kanji_token(feature: &NjdFeature) -> bool {
    feature.pos != "記号" && feature.orig.chars().any(is_cjk_unified_ideograph)
}

/// 1文字の漢字からなるか。
fn is_single_kanji_token(feature: &NjdFeature) -> bool {
    let mut chars = feature.orig.chars();
    is_kanji_token(feature)
        && chars.next().is_some_and(is_cjk_unified_ideograph)
        && chars.next().is_none()
}

/// 踊り字の直前の漢字を解析し直す必要がある場合に、解析し直す漢字と、一緒に解析する後続の漢字を返す。
fn needs_reanalysis<'a>(
    odori: &NjdFeature,
    previous: &NjdFeature,
    next: Option<&'a NjdFeature>,
) -> Option<(char, Option<&'a str>)> {
    // 単独の踊り字で、直前が複数の文字からなる漢字の語の場合だけ
    if count_odori(&odori.orig) != 1
        || !is_kanji_token(previous)
        || previous.orig.chars().count() <= 1
    {
        return None;
    }
    let last_char = previous.orig.chars().last()?;
    if !is_cjk_unified_ideograph(last_char) {
        return None;
    }
    // 後続が1文字の漢字の場合は、その漢字も含めて解析し直す
    let next_kanji = next
        .filter(|next| is_single_kanji_token(next))
        .map(|next| &*next.orig);
    Some((last_char, next_kanji))
}

/// 一の字点の読みを、直前の語の最後のモーラから作る。
fn process_odoriji(odori: &mut NjdFeature, previous: &NjdFeature) {
    let read_moras = split_kana_mora(&previous.read);
    // 無声化記号はモーラとして扱わない
    let pron_source = previous.pron.replace('’', "");
    let pron_source = if pron_source.is_empty() {
        &previous.read
    } else {
        &pron_source
    };
    let pron_moras = split_kana_mora(pron_source);
    let (Some(&previous_read), Some(&previous_pron)) = (read_moras.last(), pron_moras.last())
    else {
        return;
    };
    // モーラ数を文字数に応じて分配する
    let previous_mora_size = previous.mora_size / read_moras.len() as i32;

    // ゞ/ヾ が先に現れるかで強制的に濁音化するかを判定する
    let is_forced_voiced = odori
        .orig
        .chars()
        .find(|c| "ゝヽゞヾ".contains(*c))
        .is_some_and(|c| "ゞヾ".contains(c));
    // 一の字点は直前の仮名1文字を繰り返す記号なので、拗音は清音化しない
    let is_single_grapheme_mora = !previous_read.chars().any(|c| SMALL_KANA.contains(c));

    let (read, pron) = if is_forced_voiced {
        (voiced(previous_read), voiced(previous_pron))
    } else if is_single_grapheme_mora {
        (voiceless(previous_read), voiceless(previous_pron))
    } else {
        (previous_read, previous_pron)
    };
    odori.read = read.to_owned();
    odori.pron = pron.to_owned();
    odori.mora_size = previous_mora_size;
    make_common_noun_if_symbol(odori);
}

fn voiced(mora: &str) -> &str {
    DAKUTEN_MORAS
        .iter()
        .find(|(voiceless, _)| *voiceless == mora)
        .map_or(mora, |(_, voiced)| voiced)
}

fn voiceless(mora: &str) -> &str {
    DAKUTEN_MORAS
        .iter()
        .find(|(_, voiced)| *voiced == mora)
        .map_or(mora, |(voiceless, _)| voiceless)
}

/// Pythonの`moras[len(moras) - period:]`の開始位置。
fn py_slice_start(len: usize, period: usize) -> usize {
    let start = len as isize - period as isize;
    if start >= 0 {
        start as usize
    } else {
        (len as isize + start).max(0) as usize
    }
}

/// 展開した踊り字を後続の処理で通常の形態素として扱わせるため、「名詞,一般」にする。
fn make_common_noun(feature: &mut NjdFeature) {
    feature.pos = "名詞".to_owned();
    feature.pos_group1 = "一般".to_owned();
    feature.pos_group2 = "*".to_owned();
    feature.pos_group3 = "*".to_owned();
    feature.ctype = "*".to_owned();
    feature.cform = "*".to_owned();
}

fn make_common_noun_if_symbol(feature: &mut NjdFeature) {
    if feature.pos == "記号" {
        make_common_noun(feature);
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::super::test_util::parse_nodes;

    #[rstest]
    #[case("サマザマ", Some(2))]
    #[case("ジョジョ", Some(1))]
    #[case("フクフク", Some(2))]
    #[case("ゼンシンゼンシン", Some(4))]
    #[case("ジョ", None)]
    #[case("ケッコン", None)]
    fn detect_odori_unit_works(#[case] read: &str, #[case] expected: Option<usize>) {
        assert_eq!(expected, super::detect_odori_unit(read));
    }

    #[rstest]
    // なゝ樹
    #[case(
        "な 助詞 終助詞 * * な ナ ナ 1 1 動詞%F2@0/形容詞%F1 -1
         ゝ 記号 読点 * * ゝ 、 、 0 0 * 0
         樹 名詞 一般 * * 樹 キ キ 1 1 C3 0",
        &[],
        &[("ナ", "ナ", 1, 1, -1, "助詞"), ("ナ", "ナ", 1, 0, 0, "名詞"), ("キ", "キ", 1, 1, 0, "名詞")],
    )]
    // 金子みすゞ
    #[case(
        "金子 名詞 固有名詞 * * 金子 カネコ カネコ 0 3 C1 -1
         みす 名詞 固有名詞 * * みす ミス ミス 1 2 C4 0
         ゞ 記号 読点 * * ゞ 、 、 0 0 * 0",
        &[],
        &[("カネコ", "カネコ", 3, 0, -1, "名詞"), ("ミス", "ミス", 2, 1, 0, "名詞"), ("ズ", "ズ", 1, 0, 0, "名詞")],
    )]
    // づゝ
    #[case(
        "づ フィラー * * * づ ヅ ヅ 1 1 * -1
         ゝ 記号 読点 * * ゝ 、 、 0 0 * 0",
        &[],
        &[("ヅ", "ヅ", 1, 1, -1, "フィラー"), ("ツ", "ツ", 1, 0, 0, "名詞")],
    )]
    // ぶゞ漬け
    #[case(
        "ぶ フィラー * * * ぶ ブ ブ 1 1 * -1
         ゞ 記号 読点 * * ゞ 、 、 0 0 * 0
         漬け 名詞 一般 * * 漬け ヅケ ヅケ 1 2 C3 0",
        &[],
        &[("ブ", "ブ", 1, 1, -1, "フィラー"), ("ブ", "ブ", 1, 0, 0, "名詞"), ("ヅケ", "ヅケ", 2, 1, 0, "名詞")],
    )]
    // バナヽ
    #[case(
        "バナ フィラー * * * バナ バナ バナ 0 2 * -1
         ヽ 記号 読点 * * ヽ 、 、 0 0 * 0",
        &[],
        &[("バナ", "バナ", 2, 0, -1, "フィラー"), ("ナ", "ナ", 1, 0, 0, "名詞")],
    )]
    // 愛々
    #[case(
        "愛 名詞 一般 * * 愛 アイ アイ 1 2 C3 -1
         々 記号 読点 * * 々 、 、 0 0 * 0",
        &[],
        &[("アイ", "アイ", 2, 1, -1, "名詞"), ("アイ", "アイ", 2, 1, 1, "名詞")],
    )]
    // 咲々
    #[case(
        "咲 名詞 一般 * * 咲 サキ サキ 1 2 C4 -1
         々 記号 読点 * * 々 、 、 0 0 * 0",
        &[],
        &[("サキ", "サキ", 2, 1, -1, "名詞"), ("サキ", "サキ", 2, 1, 1, "名詞")],
    )]
    // 結婚式々場
    #[case(
        "結婚式 名詞 一般 * * 結婚式 ケッコンシキ ケッコンシ’キ 3 6 C1 -1
         々 記号 読点 * * 々 、 、 0 0 * 0
         場 名詞 接尾 * * 場 ジョウ ジョー 1 2 C4 1",
        &[("式場", "式場 名詞 一般 * * 式場 シキジョウ シ’キジョー 0 4 C2 -1")],
        &[("ケッコンシキ", "ケッコンシ’キ", 6, 3, -1, "名詞"), ("シキジョウ", "シ’キジョー", 4, 0, 1, "名詞")],
    )]
    // 学生々活
    #[case(
        "学生 名詞 一般 * * 学生 ガクセイ ガク’セー 0 4 C2 -1
         々 記号 読点 * * 々 、 、 0 0 * 0
         活 名詞 一般 * * 活 カツ カツ 1 2 C3 0",
        &[("生活", "生活 名詞 サ変接続 * * 生活 セイカツ セーカツ 0 4 C2 -1")],
        &[("ガクセイ", "ガク’セー", 4, 0, -1, "名詞"), ("セイカツ", "セーカツ", 4, 0, 1, "名詞")],
    )]
    // 民主々義
    #[case(
        "民主 名詞 一般 * * 民主 ミンシュ ミンシュ 1 3 C2 -1
         々 記号 読点 * * 々 、 、 0 0 * 0
         義 名詞 固有名詞 * * 義 ヨシ ヨシ 1 2 C3 0",
        &[("主義", "主義 名詞 一般 * * 主義 シュギ シュギ 1 2 C3 -1")],
        &[("ミンシュ", "ミンシュ", 3, 1, -1, "名詞"), ("シュギ", "シュギ", 2, 1, 1, "名詞")],
    )]
    // 叙々々苑
    #[case(
        "叙 名詞 サ変接続 * * 叙 ジョ ジョ 0 1 C3 -1
         々々 記号 読点 * * 々々 、 、 0 0 * 0
         苑 名詞 一般 * * 苑 エン エン 1 2 C3 0",
        &[],
        &[("ジョ", "ジョ", 1, 0, -1, "名詞"), ("ジョジョ", "ジョジョ", 2, 0, 1, "名詞"), ("エン", "エン", 2, 1, 0, "名詞")],
    )]
    // 叙々々々苑
    #[case(
        "叙 名詞 サ変接続 * * 叙 ジョ ジョ 0 1 C3 -1
         々々 記号 読点 * * 々々 、 、 0 0 * 0
         々 記号 読点 * * 々 、 、 0 0 * 0
         苑 名詞 一般 * * 苑 エン エン 1 2 C3 0",
        &[],
        &[("ジョ", "ジョ", 1, 0, -1, "名詞"), ("ジョジョ", "ジョジョ", 2, 0, 1, "名詞"), ("ジョ", "ジョ", 1, 0, 1, "名詞"), ("エン", "エン", 2, 1, 0, "名詞")],
    )]
    // 叙々々々々々苑
    #[case(
        "叙 名詞 サ変接続 * * 叙 ジョ ジョ 0 1 C3 -1
         々々々々々 記号 読点 * * 々々々々々 、 、 0 0 * 0
         苑 名詞 一般 * * 苑 エン エン 1 2 C3 0",
        &[],
        &[("ジョ", "ジョ", 1, 0, -1, "名詞"), ("ジョジョジョジョジョ", "ジョジョジョジョジョ", 5, 0, 1, "名詞"), ("エン", "エン", 2, 1, 0, "名詞")],
    )]
    // 複々々々線
    #[case(
        "複 名詞 一般 * * 複 フク フ’ク 2 2 C3 -1
         々々 記号 読点 * * 々々 、 、 0 0 * 0
         々 記号 読点 * * 々 、 、 0 0 * 0
         線 名詞 一般 * * 線 セン セン 1 2 C4 0",
        &[],
        &[("フク", "フ’ク", 2, 2, -1, "名詞"), ("フクフク", "フ’クフ’ク", 4, 2, 1, "名詞"), ("フク", "フ’ク", 2, 2, 1, "名詞"), ("セン", "セン", 2, 1, 0, "名詞")],
    )]
    // 今日も前進々々
    #[case(
        "今日 名詞 副詞可能 * * 今日 キョウ キョー 1 2 C3 -1
         も 助詞 係助詞 * * も モ モ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/副詞%F2@0/助詞%F2@0 1
         前進 名詞 サ変接続 * * 前進 ゼンシン ゼンシン 0 4 C2 0
         々々 記号 読点 * * 々々 、 、 0 0 * 0",
        &[],
        &[("キョウ", "キョー", 2, 1, -1, "名詞"), ("モ", "モ", 1, 0, 1, "助詞"), ("ゼンシン", "ゼンシン", 4, 0, 0, "名詞"), ("ゼンシン", "ゼンシン", 4, 0, 1, "名詞")],
    )]
    // 部分々々
    #[case(
        "部分 名詞 一般 * * 部分 ブブン ブブン 1 3 C1 -1
         々々 記号 読点 * * 々々 、 、 0 0 * 0",
        &[],
        &[("ブブン", "ブブン", 3, 1, -1, "名詞"), ("ブブン", "ブブン", 3, 1, 1, "名詞")],
    )]
    // 其他々々
    #[case(
        "其 連体詞 * * * 其 ソノ ソノ 2 2 * -1
         他 名詞 非自立 * * 他 ホカ ホカ 0 2 C3 1
         々々 記号 読点 * * 々々 、 、 0 0 * 0",
        &[],
        &[("ソノ", "ソノ", 2, 2, -1, "連体詞"), ("ホカ", "ホカ", 2, 0, 1, "名詞"), ("ソノホカ", "ソノホカ", 4, 2, 1, "名詞")],
    )]
    // やっほー！元気かな？ヾ(≧▽≦)ﾉ
    #[case(
        "やっほー 名詞 一般 * * やっほー ヤッホー ヤッホー 1 4 C1 -1
         ！ 記号 一般 * * ！ ！ ！ 0 0 * 0
         元気 名詞 形容動詞語幹 * * 元気 ゲンキ ゲンキ’ 1 3 C1 0
         か 助詞 副助詞／並立助詞／終助詞 * * か カ カ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0 1
         な 助詞 終助詞 * * な ナ ナ 1 1 動詞%F2@0/形容詞%F1 1
         ？ 記号 一般 * * ？ ？ ？ 0 0 * 0
         ヾ 記号 読点 * * ヾ 、 、 0 0 * 0
         （≧▽≦） 記号 読点 * * （≧▽≦） 、 、 0 0 * 0
         ノ 記号 一般 * * ノ ノ ノ 1 1 * 0",
        &[],
        &[("ヤッホー", "ヤッホー", 4, 1, -1, "名詞"), ("！", "！", 0, 0, 0, "記号"), ("ゲンキ", "ゲンキ’", 3, 1, 0, "名詞"), ("カ", "カ", 1, 0, 1, "助詞"), ("ナ", "ナ", 1, 1, 1, "助詞"), ("？", "？", 0, 0, 0, "記号"), ("、", "、", 0, 0, 0, "記号"), ("、", "、", 0, 0, 0, "記号"), ("ノ", "ノ", 1, 1, 0, "記号")],
    )]
    // 人。々
    #[case(
        "人 名詞 一般 * * 人 ヒト ヒ’ト 0 2 C3 -1
         。 記号 読点 * * 。 、 、 0 0 * 0
         々 記号 読点 * * 々 、 、 0 0 * 0",
        &[],
        &[("ヒト", "ヒ’ト", 2, 0, -1, "名詞"), ("、", "、", 0, 0, 0, "記号"), ("、", "、", 0, 0, 0, "記号")],
    )]
    // 人は々
    #[case(
        "人 名詞 一般 * * 人 ヒト ヒ’ト 0 2 C3 -1
         は 助詞 係助詞 * * は ハ ワ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/助詞%F2@0 1
         々 記号 読点 * * 々 、 、 0 0 * 0",
        &[],
        &[("ヒト", "ヒ’ト", 2, 0, -1, "名詞"), ("ハ", "ワ", 1, 0, 1, "助詞"), ("、", "、", 0, 0, 0, "記号")],
    )]
    // じょゝ
    #[case(
        "じょ フィラー * * * じょ ジョ ジョ 1 1 * -1
         ゝ 記号 読点 * * ゝ 、 、 0 0 * 0",
        &[],
        &[("ジョ", "ジョ", 1, 1, -1, "フィラー"), ("ジョ", "ジョ", 1, 0, 0, "名詞")],
    )]
    // ちゅゞ
    #[case(
        "ちゅ 副詞 * * * ちゅ チュ チュ 1 1 * -1
         ゞ 記号 読点 * * ゞ 、 、 0 0 * 0",
        &[],
        &[("チュ", "チュ", 1, 1, -1, "副詞"), ("ヂュ", "ヂュ", 1, 0, 0, "名詞")],
    )]
    // ゝ
    #[case(
        "ゝ 記号 読点 * * ゝ 、 、 0 0 * -1",
        &[],
        &[("、", "、", 0, 0, -1, "記号")],
    )]
    // かゝ゜
    #[case(
        "か 助詞 副助詞／並立助詞／終助詞 * * か カ カ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0 -1
         ゝ 記号 読点 * * ゝ 、 、 0 0 * 0
         ゜ 記号 読点 * * ゜ 、 、 0 0 * 0",
        &[],
        &[("カ", "カ", 1, 0, -1, "助詞"), ("カ", "カ", 1, 0, 0, "名詞"), ("、", "、", 0, 0, 0, "記号")],
    )]
    fn process_odori_features_works(
        #[case] features: &str,
        #[case] reanalyzed: &[(&str, &str)],
        #[case] expected: &[(&str, &str, i32, i32, i32, &str)],
    ) {
        let actual = super::process_odori_features(parse_nodes(features), &mut |text| {
            let (_, features) = reanalyzed
                .iter()
                .find(|(t, _)| *t == text)
                .unwrap_or_else(|| panic!("unexpected reanalysis: {text}"));
            Ok(parse_nodes(features))
        })
        .unwrap();
        assert_eq!(
            expected,
            actual
                .iter()
                .map(|f| (
                    &*f.read,
                    &*f.pron,
                    f.mora_size,
                    f.acc,
                    f.chain_flag,
                    &*f.pos,
                ))
                .collect::<Vec<_>>(),
        );
    }
}
