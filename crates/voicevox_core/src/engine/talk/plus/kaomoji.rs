// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)
// (`pyopenjtalk/_kaomoji.py`: `find_kaomoji_parts`, `silence_kaomoji`,
// `exclude_kaomoji_from_numeral_processing`)

//! 顔文字を、読みも発音も持たない1つの要素にまとめる。
//!
//! pyopenjtalk-plusはNJDの後で顔文字の範囲のノードを1つの無音のノードに置き換え、
//! `make_label`で発音が空のノードを除く。こちらのOpen JTalkのバインディングはMeCabの
//! 特徴量文字列しか返さない（形態素の文字位置・未知語の印・辞書の番号を持たない）ので、
//! 次のように近似する。
//!
//! - 文字位置: MeCabの特徴量の表層形を、`text2mecab`で正規化した本文に順に照合して求める。
//! - 未知語: 特徴量が12列未満のもの（未知語処理の特徴量は8列）。
//! - ユーザー辞書の語: 原形の列（8列目）が`*`のもの。VOICEVOXのユーザー辞書は原形を`*`で
//!   登録する（`UserDictWord::to_mecab_format`）。
//! - 顔文字の範囲は、NJDの前にMeCabの特徴量の段階で1つの`記号,読点`の特徴量にまとめ、
//!   NJDの後で無音のノードに置き換える（[`mark_kaomoji_silent`]）。pyopenjtalk-plusでは
//!   顔文字の中の語（「ω」「ノ」など）もNJDを通るが、こちらではNJDに渡さない。そのため
//!   `exclude_kaomoji_from_numeral_processing`（顔文字の中の数字・英字を数詞の処理から外す）
//!   は、この置き換えに含まれる。NJDが顔文字の境界をまたいで語を結合したかどうか
//!   （pyopenjtalk-plusの`njd_feature_char_spans`による判定）は、MeCabの特徴量が境界を
//!   またぐかどうかで判定する。

use std::{collections::BTreeSet, sync::LazyLock};

use open_jtalk::NjdFeature;
use regex::Regex;

use super::{itaiji_map::tsuyoji, known_symbols::known_symbol_feature};

const EYES: &str = "・･ﾟ゜°•●○◕╹>＞<＜≧≦∩＾^;；'＇‘’｀´`￣-˘ಠ◉б≖⌒ゝゞ〃ﾉノ/／＼\\˙◔◠◡☉⊙꒪ↀ⩌òóŏÒÓìíˊˋ˃˂❛◍º＝=╯╰◞◟☋☌☍✹☆★へつ↑↓◜◝՞Φ¯‾ᴖ⁰Ծ✖✘l*+ーಥ눈╮╭ಲ൭ˇᵕᴗ";
const MOUTHS: &str = "ω∀▽∇Ддεз﹏ヮ◡ㅅ益皿_＿‿ーｰ-～〜~︿ᵕᴗᗜ⊖△◇○〇口ρڡڼਊة㉦⌓³₃⊱ᯅ⤚。oｏOＯㅂ‸︶⌣⁀Δ∧ᆺ▿﹃꒳Θワx0.ȏᗨへエළ╯ᴥｪェꞈˬ";
const ARMS: &str =
    "ヾヽゞゝﾉノつっ⊂⊃∩╰╯╭╮┌┐└┘┗┛┏┓」｢｣⌒✌☝☛☚٩۶งว੭ʅʃᕦᕤ＼／ιυщوΣ∑mopqdbcvεσρぅ彡卍ฅ∪լ∠_「ｼﾞ";
const ORNAMENTS: &str = " \u{3000}*＊｡。.,，､、:：♡♥❤✧✿❀๑〃＠@ｰー\u{35c}\u{300}\u{301}\u{e34}ﾞ゛\u{3099}ˇ▔▰灬˶⸝︶⌣|＃#∗❁〄☞☜\u{335}\u{311}\u{323}\u{325}\u{e38}⁾\u{e31}∞⋈✗ㆀ†\u{942}\u{941}∽\u{35f}\u{35e}~";
const PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('（', '）'),
    ('⎛', '⎞'),
    ('꒰', '꒱'),
    ('₍', '₎'),
    ('ʕ', 'ʔ'),
    ('∪', '∪'),
    ('⎝', '⎠'),
    ('|', ')'),
    ('｜', '）'),
];
const STRONG_MOUTHS: &str = "ω∀▽∇Ддεз﹏ヮᴥｪェᵕ꒳";
const BARS: &str = "-－−ーｰ―─━═╌┄";

/// 顔文字の、正規化前の文字位置での範囲。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KaomojiPart {
    /// 顔の本体の開始。
    pub(crate) core_start: usize,
    /// 顔の本体の終端。
    pub(crate) core_end: usize,
    /// 腕・持ち物を含む全体の開始。
    pub(crate) start: usize,
    /// 腕・持ち物を含む全体の終端。
    pub(crate) end: usize,
}

/// 正規化前の文字位置（`char`単位）で、顔の本体と腕・持ち物を含む範囲を返す。
pub(crate) fn find_kaomoji_parts(text: &str) -> Vec<KaomojiPart> {
    let text = text.chars().collect::<Vec<_>>();
    let len = text.len();
    let mut parts = vec![];
    let mut consumed = 0;
    for index in 0..len {
        if index < consumed {
            continue;
        }
        let original = text[index];
        let extra_end = extra_face_end(&text, index);
        let mut end = extra_end;
        if end.is_none()
            && let Some(closing) = pair_closing(original)
        {
            let mut depth = 0;
            for (at, &raw) in text.iter().enumerate().take(index + 50).skip(index + 1) {
                let c = fold(raw);
                if "⎛꒰₍\n\r".contains(c) {
                    break;
                }
                if c == '(' {
                    depth += 1;
                }
                if ")⎞꒱₎ʔ⎠".contains(c) || (original == '∪' && c == '∪') {
                    if c == ')' && depth > 0 {
                        depth -= 1;
                    } else {
                        if raw == closing || (original == '⎝' && c == '⎞') {
                            end = Some(at + 1);
                        }
                        break;
                    }
                }
            }
        }
        let Some(end) = end else {
            continue;
        };
        let (mut start, mut extent_end) = (index, end);
        let lower = (consumed as isize - 1).max(index as isize - 17);
        for at in ((lower + 1).max(0) as usize..index).rev() {
            let c = text[at];
            if !arm(c)
                || "ｼﾞ".contains(c)
                || (letter(c) && ((at > 0 && letter(text[at - 1])) || letter(text[at + 1])))
            {
                break;
            }
            start = at;
        }
        let mut last = None;
        for at in end..len.min(end + 16) {
            let c = text[at];
            let follows_shi = last.is_none_or(|last| "ﾉｼ".contains(last));
            if !(arm(c) || ("ｼﾞ".contains(c) && follows_shi))
                || (letter(c) && (letter(text[at - 1]) || (at + 1 < len && letter(text[at + 1]))))
            {
                break;
            }
            extent_end = at + 1;
            last = Some(c);
        }
        if extra_end != Some(end)
            && !body(
                &text[index + 1..end - 1]
                    .iter()
                    .map(|&c| fold(c))
                    .collect::<Vec<_>>(),
                start != index || extent_end != end || "|｜".contains(original),
            )
        {
            continue;
        }
        // 濁点・結合文字・異体字セレクタも顔文字の範囲に含める
        while extent_end < len && decoration(text[extent_end]) {
            extent_end += 1;
        }
        if text[extent_end - 1] == '\u{200d}' || (start > 0 && text[start - 1] == '\u{200d}') {
            continue;
        }
        let (start, extent_end) = extend_objects(&text, index, end, start, extent_end);
        parts.push(KaomojiPart {
            core_start: index,
            core_end: end,
            start,
            end: extent_end,
        });
        consumed = end;
    }
    if parts.is_empty() {
        return parts;
    }

    // 本文の引用符は、顔の外側にある対応の取れた組だけを残す
    let mut quotes = BTreeSet::new();
    let mut stack = vec![];
    for (index, &c) in text.iter().enumerate() {
        if parts
            .iter()
            .any(|part| part.core_start <= index && index < part.core_end)
        {
            continue;
        }
        if "「『".contains(c) {
            stack.push((index, c));
        } else if let Some(&(open, open_c)) = stack.last()
            && matches!((open_c, c), ('「', '」') | ('『', '』'))
        {
            stack.pop();
            quotes.insert(open);
            quotes.insert(index);
        }
    }
    parts
        .into_iter()
        .map(|part| KaomojiPart {
            start: quotes
                .iter()
                .filter(|&&q| part.start <= q && q < part.core_start)
                .map(|&q| q + 1)
                .fold(part.start, usize::max),
            end: quotes
                .iter()
                .filter(|&&q| part.core_end <= q && q < part.end)
                .fold(part.end, |end, &q| end.min(q)),
            ..part
        })
        .collect()
}

fn pair_closing(c: char) -> Option<char> {
    PAIRS
        .iter()
        .find(|&&(open, _)| open == c)
        .map(|&(_, close)| close)
}

fn starts_with(text: &[char], at: usize, prefix: &str) -> bool {
    prefix
        .chars()
        .enumerate()
        .all(|(i, c)| text.get(at + i) == Some(&c))
}

/// `text[start..end]`（Pythonのスライスと同じく、範囲外は切り詰める）。
fn slice(text: &[char], start: usize, end: usize) -> &[char] {
    let end = end.min(text.len());
    &text[start.min(end)..end]
}

/// 手で囲んだ顔、つながった顔、輪郭のない定型の顔の終端を返す。
fn extra_face_end(text: &[char], start: usize) -> Option<usize> {
    if !">＞눈ᓀ/(（ʕ".contains(text[start]) {
        return None;
    }
    let tail = slice(text, start, start + 100);
    for form in [">ω<", "＞ω＜", "눈_눈", "ᓀ‸ᓂ", "/ᐠ｡ꞈ｡ᐟ\\"] {
        if starts_with(tail, 0, form) {
            return Some(start + form.chars().count());
        }
    }
    for head in ["(:3[", "(¦3[", "('､3[", "('、3["] {
        if starts_with(tail, 0, head) {
            let head_len = head.chars().count();
            for (index, &c) in slice(tail, head_len, head_len + 9).iter().enumerate() {
                if c == ']' && (2..=8).contains(&index) {
                    return Some(start + head_len + index + 1);
                }
                if !"_＿▓".contains(c) {
                    break;
                }
            }
        }
    }
    if !"(（ʕ".contains(tail[0]) {
        return None;
    }
    let folded = tail.iter().map(|&c| fold(c)).collect::<Vec<_>>();
    let len = folded.len();
    // 「(ﾉ)ω(ヾ)」では、左右の手と中央の口を一緒に認識する
    if len > 5 && "ﾉノ/".contains(folded[1]) && folded[2] == ')' {
        let opening = slice(&folded, 3, 20)
            .iter()
            .position(|&c| c == '(')
            .map(|i| i + 3);
        if let Some(opening) = opening
            && opening + 2 < len
            && covering_pair(folded[1], folded[opening + 1])
            && folded[opening + 2] == ')'
        {
            let middle = &folded[3..opening];
            if (middle.len() == 1 && STRONG_MOUTHS.contains(middle[0])) || body(middle, true) {
                return Some(start + opening + 3);
            }
        }
    }
    // 手つなぎや頬つつきは、接続する両端に顔がある場合だけ認める
    let find_delimiter =
        |cursor: usize| (cursor..len.min(cursor + 21)).find(|&i| "()人".contains(folded[i]));
    let mut cursor = 1;
    let mut last_end = None;
    let left_end = find_delimiter(cursor)?;
    if !social_body(&folded[cursor..left_end], false, folded[left_end] == '(') {
        return None;
    }
    let mut delimiter = folded[left_end];
    cursor = left_end + 1;
    for _ in 0..7 {
        let mut missing_left = delimiter == ')';
        if delimiter == ')' {
            if starts_with(&folded, cursor, "人(") {
                cursor += 2;
                missing_left = false;
            } else if cursor < len && "σっ".contains(folded[cursor]) {
                cursor += 1;
                if cursor < len && "\"”".contains(folded[cursor]) {
                    cursor += 1;
                }
                if cursor >= len || folded[cursor] != ')' {
                    break;
                }
                cursor += 1;
            } else if starts_with(&folded, cursor, ")") {
                cursor += 1;
            }
        }
        let Some(right_end) = find_delimiter(cursor) else {
            break;
        };
        if folded[right_end] != ')' || !social_body(&folded[cursor..right_end], missing_left, false)
        {
            break;
        }
        cursor = right_end + 1;
        last_end = Some(start + cursor);
        delimiter = ')';
    }
    last_end
}

/// 連結された顔文字の本体を、輪郭と目の省略を考慮して判定する。
fn social_body(text: &[char], missing_left: bool, missing_right: bool) -> bool {
    if text.is_empty() || text.len() > 20 {
        return false;
    }
    if text.iter().any(char::is_ascii_alphanumeric) {
        return !missing_left && !missing_right && body(text, false);
    }
    // 輪郭を省いた連結では口を確認し、「つー>ﾟ)))≫彡」の釣り竿を途中まで2つ目の顔にしない
    if (missing_left || missing_right) && !text.iter().any(|&c| STRONG_MOUTHS.contains(c)) {
        return false;
    }
    if body(text, true) {
        return true;
    }
    let clean = strip(text, |c| " *｡".contains(c));
    let Some((&first, rest)) = clean.split_first() else {
        return false;
    };
    let (&last, init) = clean.split_last().expect("not empty");
    (missing_left
        && STRONG_MOUTHS.contains(first)
        && rest.iter().all(|&c| allowed(c))
        && rest.iter().any(|&c| EYES.contains(c)))
        || (missing_right
            && STRONG_MOUTHS.contains(last)
            && init.iter().all(|&c| allowed(c))
            && init.iter().any(|&c| EYES.contains(c)))
}

fn strip(text: &[char], pred: impl Fn(char) -> bool) -> &[char] {
    let start = text.iter().position(|&c| !pred(c)).unwrap_or(text.len());
    let end = text
        .iter()
        .rposition(|&c| !pred(c))
        .map_or(start, |i| i + 1);
    &text[start..end]
}

/// Pythonの`str.isspace`。
fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn eq_str(text: &[char], s: &str) -> bool {
    text.len() == s.chars().count() && starts_with(text, 0, s)
}

/// 目と口の組み合わせから顔の本体が成立するかを調べる。
///
/// 英数字を含む形は、変数や数式を顔と取り違えないよう、確認できる目と口の組に限定する。
/// `text`は判定用に全角記号を半角へ戻した顔の内部、`has_arms`は顔の外に腕があるか。
fn body(text: &[char], has_arms: bool) -> bool {
    if !(2..=48).contains(&text.len()) {
        return false;
    }
    let t = strip(text, is_py_space);
    let clean = t
        .iter()
        .copied()
        .filter(|&c| !" *｡´`｀〃ᐢ′‵๑".contains(c))
        .collect::<Vec<_>>();
    if clean.len() >= 2 && covering_pair(clean[0], clean[clean.len() - 1]) {
        if clean.len() == 2 && !clean[0].is_ascii() {
            return true;
        }
        if clean.len() > 3 && !clean[0].is_ascii() && body(&clean[1..clean.len() - 1], false) {
            return true;
        }
        if clean.len() == 3
            && (STRONG_MOUTHS.contains(clean[1])
                || (!clean[0].is_ascii() && "-_ー".contains(clean[1])))
        {
            return true;
        }
    }
    if t.iter().all(|&c| "-ー= ".contains(c)) {
        return false;
    }
    if t.contains(&'三') {
        return t.iter().filter(|&&c| c == '三').count() <= 2
            && t.split(|&c| c == '三').all(|part| body(part, has_arms));
    }
    if t.len() == 3
        && "TtOo0xXQq3".contains(t[0])
        && t[0].eq_ignore_ascii_case(&t[2])
        && (t[1] == '_'
            || ("TtOoXxQq".contains(t[0]) && "ω﹏".contains(t[1]))
            || ("TtQq".contains(t[0]) && t[1] == '.'))
    {
        return true;
    }
    if eq_str(t, "><")
        || (has_arms
            && ["・・", "･･", "・・。", "。。", "_ _"]
                .iter()
                .any(|s| eq_str(t, s)))
    {
        return true;
    }
    let mut eyes = t
        .iter()
        .copied()
        .filter(|&c| !" *｡".contains(c))
        .collect::<Vec<_>>();
    if eyes.len() >= 4 && eyes[0] == 'o' && eyes[eyes.len() - 1] == 'o' {
        eyes = eyes[1..eyes.len() - 1].to_vec();
    }
    if eyes.len() == 2
        && eyes[0] == eyes[1]
        && ("^˘ˇ".contains(eyes[0]) || (has_arms && "･・".contains(eyes[0])))
    {
        return true;
    }
    let t_str = t.iter().collect::<String>();
    if t_str.contains("ε:") && t_str.replace("ε:", "").chars().all(|c| arm(c) || c == ' ') {
        return true;
    }
    for head in [":3", ":D", ":0", "ε:", "'､3", "'、3"] {
        if let Some(rest) = t_str.strip_prefix(head)
            && rest.chars().all(|c| arm(c) || c == ' ')
        {
            return true;
        }
    }
    if t.contains(&'(') || t.contains(&')') {
        let t = t
            .iter()
            .copied()
            .filter(|&c| c != '(' && c != ')')
            .collect::<Vec<_>>();
        return body(&t, has_arms);
    }
    let mut text = text.to_vec();
    if text.iter().any(char::is_ascii_alphanumeric) {
        let is_hands = t
            .iter()
            .all(|&c| allowed(c) || MOUTHS.contains(c) || "opcqbdl".contains(c));
        let is_o_mouth = t.contains(&'o')
            && t.iter().all(|&c| allowed(c) || c == 'o')
            && t.iter().any(|&c| "^＾".contains(c));
        if !is_hands && !is_o_mouth {
            return false;
        }
        let other_mouth = text.iter().any(|&c| c != 'o' && MOUTHS.contains(c));
        text.retain(|&c| !(is_hands && ("pcqbdP".contains(c) || (c == 'o' && other_mouth))));
    }
    if text.iter().any(|&c| "≡Ξ三".contains(c)) {
        let chunks = text.split(|&c| "≡Ξ三".contains(c)).collect::<Vec<_>>();
        return chunks.len() <= 3 && chunks.iter().all(|part| body(part, has_arms));
    }
    let one_eye = has_arms || text.iter().any(|&c| "ヾヽゞゝﾉノつっ⊂⊃".contains(c));
    for (index, &c) in text.iter().enumerate() {
        let (before, after) = (&text[..index], &text[index + 1..]);
        if MOUTHS.contains(c) && before.iter().chain(after).all(|&x| allowed(x)) {
            let before = before.iter().any(|&x| EYES.contains(x));
            let after = after.iter().any(|&x| EYES.contains(x));
            if (before && after) || (one_eye && "ω∀▽∇Ддεзヮ益皿◡".contains(c) && (before || after))
            {
                return true;
            }
        }
    }
    false
}

/// 口や目を覆う左右の手の組かを調べる。
fn covering_pair(a: char, b: char) -> bool {
    matches!(
        (a, b),
        ('⊃', '⊂') | ('つ', '⊂') | ('っ', 'c') | ('p', 'q') | ('q', 'p') | ('∩', '∩') | ('ฅ', 'ฅ')
    ) || ("ﾉノ/".contains(a) && "ﾉノヾ\\".contains(b))
}

/// 全角英数字・記号を形の判定用に半角へ戻す。
fn fold(c: char) -> char {
    match c {
        '！'..='～' => char::from_u32(c as u32 - 0xFEE0).expect("ASCII"),
        '−' => '-',
        '\u{3000}' => ' ',
        c => c,
    }
}

/// 腕に使える文字かを調べる。
fn arm(c: char) -> bool {
    ARMS.contains(c) || ARMS.contains(fold(c)) || "/\\".contains(c)
}

/// 腕と混同しやすい英数字やギリシャ文字かを調べる。
fn letter(c: char) -> bool {
    let c = fold(c);
    (c.is_ascii_alphanumeric() || c == '_')
        || (('\u{370}'..='\u{52f}').contains(&c) && c.is_alphabetic())
}

/// 顔の装飾・目・腕に使える文字かを調べる。
fn allowed(c: char) -> bool {
    EYES.contains(c)
        || arm(c)
        || ORNAMENTS.contains(c)
        || decoration(c)
        || "🎀💢💦\u{200b}ლᐢˇP".contains(c)
}

/// 顔に続く結合文字や異体字セレクタかを調べる。
fn decoration(c: char) -> bool {
    static MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{M}$").unwrap());
    "ﾞﾟ\u{200d}".contains(c)
        || ('\u{fe00}'..='\u{fe0f}').contains(&c)
        || MARK.is_match(c.encode_utf8(&mut [0; 4]))
}

/// Pythonの`str.isnumeric`。
///
/// 漢数字のうち、よく使われるものだけを数として扱う近似。
fn is_py_numeric(c: char) -> bool {
    c.is_numeric() || "〇一二三四五六七八九十百千万億兆零壱弐参拾廿卅".contains(c)
}

/// 顔の手に接続した団子・飲み物・棒の先の図形を含めた範囲を返す。
///
/// 「○○○円」「☆5個」のように数や単位が続く図形は本文として残す。
fn extend_objects(
    text: &[char],
    core_start: usize,
    core_end: usize,
    mut start: usize,
    mut end: usize,
) -> (usize, usize) {
    for is_left in [false, true] {
        let existing = if is_left {
            &text[start..core_start]
        } else {
            &text[core_end..end]
        };
        let tail = if is_left {
            text[..start].iter().rev().copied().collect::<Vec<_>>()
        } else {
            text[end..].to_vec()
        };
        let has_hand = existing
            .iter()
            .any(|&c| "つっ⊂⊃ﾉノヾヽoｏcｃ_＿/／＼\\".contains(c));
        let count = object_length(&tail, has_hand);
        if count > 0 {
            if is_left {
                start -= count;
            } else {
                end += count;
            }
        }
    }
    (start, end)
}

/// 手の先の持ち物が成立する場合に、その文字数を返す（成立しない場合は0）。
///
/// `text`は腕の外側から順に並べた文字列、`has_hand`は顔の輪郭の外に手があるか。
fn object_length(text: &[char], has_hand: bool) -> usize {
    const CIRCLES: &str = "○●◎〇◯￮";
    const STARS: &str = "☆★✿❀";
    const STEAM: &str = "~～〜”\"";
    let len = text.len();
    let at = |i: usize| text.get(i).copied();
    let is_in = |i: usize, set: &str| at(i).is_some_and(|c| set.contains(c));

    if !has_hand && strip_start(text, " \u{3000}").first() != Some(&'︻') {
        return 0;
    }
    let mut index = 0;
    while index < 2 && is_in(index, " \u{3000}") {
        index += 1;
    }
    let bars_start = index;
    while index - bars_start < 12 && is_in(index, BARS) {
        index += 1;
    }
    let bars = index - bars_start;
    let shape_start = index;
    if index >= len {
        return 0;
    }
    let c = text[index];
    if bars > 0 && CIRCLES.contains(c) {
        while index - shape_start < 6 && is_in(index, CIRCLES) {
            index += 1;
        }
        if index - shape_start < 2 || is_in(index, CIRCLES) {
            return 0;
        }
        if is_in(index, BARS) {
            index += 1;
        }
    } else if bars > 0 && STARS.contains(c) {
        while index - shape_start < 8 && is_in(index, STARS) {
            index += 1;
        }
        if is_in(index, STARS) {
            return 0;
        }
        let ornament_start = index;
        while index - ornament_start < 8 && is_in(index, "。｡.ﾟ゜*＊･・:：✧✨") {
            index += 1;
        }
    } else if bars > 0 && c == '>' {
        index += 1;
        if !is_in(index, "ﾟ゜") {
            return 0;
        }
        index += 1;
        let gills_start = index;
        while index - gills_start < 8 && at(index) == Some(')') {
            index += 1;
        }
        if index == gills_start
            || index + 1 >= len
            || !"≫>".contains(text[index])
            || text[index + 1] != '彡'
        {
            return 0;
        }
        index += 2;
        let tail_end = index;
        let mut last_wave = index;
        while index - tail_end < 8 && is_in(index, "~ \u{3000}") {
            if text[index] == '~' {
                last_wave = index + 1;
            }
            index += 1;
        }
        index = last_wave;
    } else if bars > 0 && starts_with(text, index, "[二二]") {
        index += 4;
    } else if bars > 0 && starts_with(text, index, "{}") {
        index += 2;
        let mut count = 1;
        while starts_with(text, index, "@{}") && count < 4 {
            index += 3;
            count += 1;
        }
        if count < 2 {
            return 0;
        }
        if is_in(index, BARS) {
            index += 1;
        }
    } else if bars == 0 && c == '︻' {
        index += 1;
        if at(index) == Some('┻') {
            index += 1;
        }
        if !is_in(index, "┳デ") {
            return 0;
        }
        index += 1;
        let barrel_start = index;
        while index - barrel_start < 8 && is_in(index, "═━") {
            index += 1;
        }
        if index == barrel_start || at(index) != Some('一') {
            return 0;
        }
        index += 1;
    } else if bars == 0
        && index + 1 < len
        && (("￭■□".contains(c) && "PDCＰＤＣ".contains(text[index + 1]))
            || ("PDCＰＤＣ".contains(c) && "￭■□".contains(text[index + 1])))
    {
        index += 2;
        let steam_start = index;
        while index - steam_start < 4 && is_in(index, STEAM) {
            index += 1;
        }
    } else if bars == 0 && "旦且🍡☕🍵".contains(c) {
        index += 1;
        if is_in(index, "\u{fe0e}\u{fe0f}") {
            index += 1;
        }
        let steam_start = index;
        while index - steam_start < 4 && is_in(index, STEAM) {
            index += 1;
        }
        if "旦且".contains(c) && index == steam_start {
            return 0;
        }
    } else {
        return 0;
    }
    if is_in(index, "\u{fe0e}\u{fe0f}") {
        index += 1;
    }
    if at(index).is_some_and(decoration) {
        return 0;
    }
    if at(index).is_some_and(|c| ('\u{1f3fb}'..='\u{1f3ff}').contains(&c)) {
        return 0;
    }
    if let Some(&following) = strip_start(&text[index.min(len)..], " \u{3000}").first()
        && (is_py_numeric(following)
            || following.is_ascii_alphanumeric()
            || "円件個点番号年月日時分秒割％%式項口人台本桁".contains(following))
    {
        return 0;
    }
    index
}

fn strip_start<'a>(text: &'a [char], chars: &str) -> &'a [char] {
    let start = text
        .iter()
        .position(|&c| !chars.contains(c))
        .unwrap_or(text.len());
    &text[start..]
}

/// 顔文字の品詞。
const KAOMOJI: &str = "顔文字";

/// [`silence_kaomoji`]の結果。
#[derive(Debug)]
pub(crate) struct SilencedKaomoji {
    /// 顔文字の範囲を1つの`記号,読点`にまとめたMeCabの特徴量。
    pub(crate) mecab_features: Vec<String>,
    /// まとめた顔文字の表層形（入力順）。NJDの後で[`mark_kaomoji_silent`]が見分けるために使う。
    pub(crate) surfaces: Vec<String>,
}

/// MeCabの特徴量（空白を含む、`apply_mecab_rules`を適用する前のもの）のうち、顔文字の範囲を
/// 1つの`記号,読点`の特徴量にまとめる。
///
/// - `raw_text`: 異体字を置き換える前の入力。顔文字の判定はこの文字列で行う。
/// - `text`: MeCabに渡した本文（異体字を置き換えた後）。
/// - `features`: `text`のMeCabの解析結果。
/// - `normalize_for_mecab`: Open JTalkの`text2mecab`による正規化。
///
/// 顔文字が語と重なるなどして1つもまとめなかった場合や、文字位置を対応付けられなかった場合は
/// `None`を返す。
///
/// NJDに渡した後、[`mark_kaomoji_silent`]で無音のノードに置き換える。
pub(crate) fn silence_kaomoji(
    raw_text: &str,
    parts: &[KaomojiPart],
    text: &str,
    features: &[String],
    normalize_for_mecab: impl Fn(&str) -> anyhow::Result<String>,
) -> Option<SilencedKaomoji> {
    let normalized = normalize_for_mecab(text).ok()?.chars().collect::<Vec<_>>();
    let source_spans = caller_text_spans(raw_text, &normalized, &normalize_for_mecab)?;
    let morphs = morphs(features, &normalized)?;
    let accepted = accept_kaomoji_spans(parts, &source_spans, &morphs);
    if accepted.is_empty() {
        return None;
    }

    let mut result = vec![];
    let mut surfaces = vec![];
    let mut emitted = vec![false; accepted.len()];
    let mut emit = |result: &mut Vec<String>, feature: &str, (a, b): (usize, usize)| match accepted
        .iter()
        .position(|&(start, end)| start <= a && b <= end)
    {
        Some(i) => {
            if !emitted[i] {
                emitted[i] = true;
                let (start, end) = accepted[i];
                let surface = normalized[start..end].iter().collect::<String>();
                result.push(format!(
                    "{surface},記号,読点,{KAOMOJI},*,*,*,{surface},、,、,0/0,*"
                ));
                surfaces.push(surface);
            }
        }
        None => result.push(feature.to_owned()),
    };
    let units_by_feature = morphs.chunk_by(|x, y| x.feature_index == y.feature_index);
    for (feature, units) in features.iter().zip(units_by_feature) {
        let (a, b) = (units[0].span.0, units[units.len() - 1].span.1);
        let straddles = accepted
            .iter()
            .any(|&(start, end)| a < end && start < b && (a < start || b > end));
        if straddles {
            // 顔文字の境界をまたぐ記号の並びは、1文字ずつの特徴量に分ける
            for unit in units {
                emit(&mut result, &unit.feature, unit.span);
            }
        } else {
            emit(&mut result, feature, (a, b));
        }
    }
    Some(SilencedKaomoji {
        mecab_features: result,
        surfaces,
    })
}

/// [`silence_kaomoji`]がまとめた顔文字のノードを、読みと発音が空のノードに置き換える。
///
/// `surfaces`は[`SilencedKaomoji::surfaces`]。NJDは品詞や読みを書き換える（「ヾ（≧▽≦）ノ」を
/// 末尾の仮名で`フィラー`の「ノ」にするなど）ので、表層形が一致するノードを入力順に探す。
///
/// pyopenjtalk-plusと同じく、品詞は`その他,顔文字`。発音が空のノードはラベルを作る前に除く。
/// `njd_set_pronunciation`の後、[`silence_kaomoji`]がまとめた顔文字のノードを`記号,読点`に戻す。
///
/// NJDは英字を含む顔文字（「（Ｔ＿Ｔ）」）を英字の読みのフィラーに読み直すことがあり、そのままだと
/// フィラーの句の区切りの規則が当たってしまう。pyopenjtalk-plusでは顔文字の中身は記号の形態素の
/// ままNJDを通る。
pub(crate) fn restore_kaomoji_symbols(
    features: Vec<NjdFeature>,
    surfaces: &[String],
) -> Vec<NjdFeature> {
    if surfaces.is_empty() {
        return features;
    }
    let mut surfaces = surfaces.iter().peekable();
    features
        .into_iter()
        .map(|mut feature| {
            if surfaces.next_if(|&s| *s == feature.string).is_some() {
                feature.pos = "記号".to_owned();
                feature.pos_group1 = "読点".to_owned();
                feature.read = "、".to_owned();
                feature.pron = "、".to_owned();
                feature.acc = 0;
                feature.mora_size = 0;
            }
            feature
        })
        .collect()
}

pub(crate) fn mark_kaomoji_silent(
    features: Vec<NjdFeature>,
    surfaces: &[String],
) -> Vec<NjdFeature> {
    let mut surfaces = surfaces.iter().peekable();
    features
        .into_iter()
        .map(|feature| {
            if surfaces.next_if(|&s| *s == feature.string).is_some() {
                NjdFeature {
                    orig: feature.string.clone(),
                    string: feature.string,
                    pos: "その他".to_owned(),
                    pos_group1: KAOMOJI.to_owned(),
                    pos_group2: "*".to_owned(),
                    pos_group3: "*".to_owned(),
                    ctype: "*".to_owned(),
                    cform: "*".to_owned(),
                    read: String::new(),
                    pron: String::new(),
                    acc: 0,
                    mora_size: 0,
                    chain_rule: "*".to_owned(),
                    chain_flag: 1,
                }
            } else {
                feature
            }
        })
        .collect()
}

/// pyopenjtalk-plusの`MeCabMorph`のうち、顔文字の判定に使う部分。
#[derive(Debug)]
struct Morph {
    /// 元の特徴量の位置。
    feature_index: usize,
    /// 表層形を含む特徴量。記号の並びを分けたものは1文字分。
    feature: String,
    /// MeCab向けに正規化した本文上の範囲。
    span: (usize, usize),
    surface: Vec<char>,
    is_unknown: bool,
    is_registered: bool,
}

/// MeCabの特徴量に文字位置を付け、未知語にまとめられた記号の並びを1文字ずつに分ける。
///
/// pyopenjtalk-plusの`run_mecab_detailed`の詳細形態素（`_expand_symbol_morphs`）に相当する。
fn morphs(features: &[String], normalized: &[char]) -> Option<Vec<Morph>> {
    let mut morphs = vec![];
    let mut position = 0;
    for (feature_index, feature) in features.iter().enumerate() {
        let columns = feature.split(',').collect::<Vec<_>>();
        let surface = columns[0].chars().collect::<Vec<_>>();
        if surface.is_empty() {
            return None;
        }
        // MeCabが読み飛ばした文字は、次の表層形が見つかる位置まで進めて詰める
        let start = (position..=normalized.len().saturating_sub(surface.len()))
            .find(|&i| normalized[i..].starts_with(&surface))?;
        position = start + surface.len();
        let is_unknown = columns.len() < 12;
        let is_registered = !is_unknown && columns[7] == "*";
        if is_unknown && surface.len() > 1 && surface.iter().all(|c| !c.is_alphanumeric()) {
            let rest = columns[1..].join(",");
            for (offset, &c) in surface.iter().enumerate() {
                let known = known_symbol_feature(c);
                morphs.push(Morph {
                    feature_index,
                    feature: match known {
                        Some(known) => format!("{c},{known}"),
                        None => format!("{c},{rest}"),
                    },
                    span: (start + offset, start + offset + 1),
                    surface: vec![c],
                    is_unknown: known.is_none(),
                    is_registered: false,
                });
            }
        } else {
            morphs.push(Morph {
                feature_index,
                feature: feature.clone(),
                span: (start, position),
                surface,
                is_unknown,
                is_registered,
            });
        }
    }
    Some(morphs)
}

/// MeCab向けに正規化した本文の各文字に対応する、入力上の範囲を返す。
///
/// pyopenjtalk-plusの`build_caller_text_spans_by_mecab_character`に相当する。異体字の置き換え
/// （`normalize_itaiji`）の前後の2通りで照合する。ローマ数字の置き換えはこちらでは行わないので
/// 照合しない。
fn caller_text_spans(
    raw_text: &str,
    normalized: &[char],
    normalize_for_mecab: impl Fn(&str) -> anyhow::Result<String>,
) -> Option<Vec<(usize, usize)>> {
    const MAX_CHUNK_LENGTH: usize = 18;
    let is_selector = |c: char| {
        ('\u{fe00}'..='\u{fe0f}').contains(&c) || ('\u{e0100}'..='\u{e01ef}').contains(&c)
    };
    let raw = raw_text.chars().collect::<Vec<_>>();
    let mut spans = vec![];
    let mut source_start = 0;
    let mut mecab_start = 0;
    while source_start < raw.len() {
        let mut matched = None;
        let mut maximum_source_end = (source_start + MAX_CHUNK_LENGTH).min(raw.len());
        // セレクターは直前の文字と一緒に正規化し、結合先がある場合だけ取り除くCの実装とそろえる
        while maximum_source_end < raw.len() && is_selector(raw[maximum_source_end]) {
            maximum_source_end += 1;
        }
        for source_end in source_start + 1..=maximum_source_end {
            // 制御文字は`text2mecab`が読み飛ばすため、直後に続く異体字セレクターも直前の表示文字と
            // 一緒に照合する
            let mut selector_start = source_end;
            while selector_start < raw.len()
                && (u32::from(raw[selector_start]) < 0x20 || raw[selector_start] == '\x7f')
            {
                selector_start += 1;
            }
            if selector_start < raw.len() && is_selector(raw[selector_start]) {
                continue;
            }
            let chunk = &raw[source_start..source_end];
            let candidate = normalize_for_mecab(&chunk.iter().collect::<String>()).ok()?;
            if candidate.is_empty() {
                matched = Some((source_end, 0));
                break;
            }
            let itaiji = normalize_for_mecab(
                &chunk
                    .iter()
                    .map(|&c| tsuyoji(c).unwrap_or(c))
                    .collect::<String>(),
            )
            .ok()?;
            let found = [candidate, itaiji].into_iter().find_map(|candidate| {
                let candidate = candidate.chars().collect::<Vec<_>>();
                normalized[mecab_start..]
                    .starts_with(&candidate)
                    .then_some(candidate.len())
            });
            if let Some(len) = found {
                matched = Some((source_end, len));
                break;
            }
        }
        let (matched_end, matched_len) = matched?;
        // 文字数を変えずに置き換えた塊は1文字ずつ対応させる
        if matched_len == matched_end - source_start {
            spans.extend((source_start..matched_end).map(|i| (i, i + 1)));
        } else {
            spans.extend(std::iter::repeat_n(
                (source_start, matched_end),
                matched_len,
            ));
        }
        source_start = matched_end;
        mecab_start += matched_len;
    }
    (mecab_start == normalized.len()).then_some(spans)
}

/// 顔文字の候補のうち、語と重ならないものの範囲（MeCab向けに正規化した本文上）を返す。
///
/// 語の一部やユーザー辞書の語と重なる候補は除き、語と重なる腕や持ち物だけを範囲から外す。
fn accept_kaomoji_spans(
    parts: &[KaomojiPart],
    source_spans: &[(usize, usize)],
    morphs: &[Morph],
) -> Vec<(usize, usize)> {
    let mut accepted: Vec<(usize, usize)> = vec![];
    for part in parts {
        let indices = |start: usize, end: usize| {
            let mut indices = source_spans
                .iter()
                .enumerate()
                .filter(move |&(_, &(a, b))| a < end && start < b)
                .map(|(i, _)| i);
            let first = indices.next()?;
            Some((first, indices.next_back().unwrap_or(first) + 1))
        };
        let (Some((core_start, core_end)), Some((mut start, mut end))) = (
            indices(part.core_start, part.core_end),
            indices(part.start, part.end),
        ) else {
            continue;
        };
        let mut is_rejected = false;
        for morph in morphs {
            let (a, b) = morph.span;
            if b <= start || a >= end {
                continue;
            }
            let intersects_core = a < core_end && core_start < b;
            if intersects_core && (morph.is_registered || a < start || b > end) {
                is_rejected = true;
                break;
            }
            // pyopenjtalk-plusは`features[0]`（表層形）を`記号`と比べている
            let is_lexical = !morph.is_unknown
                && morph.surface != ['記', '号']
                && morph.surface.len() > 1
                && !(ARMS.contains(morph.surface[0])
                    && morph.surface[1..].iter().all(|&c| BARS.contains(c)));
            if !intersects_core && (morph.is_registered || is_lexical || a < start || b > end) {
                if b <= core_start {
                    start = start.max(b);
                } else if a >= core_end {
                    end = end.min(a);
                }
            }
        }
        // 境界をまたぐ特徴量（記号の並びとして分けられないもの）がある場合も、本文まで消さず元の
        // 解析を残す
        let straddles = |m: &Morph| {
            let (a, b) = m.span;
            a < end && start < b && (a < start || b > end)
        };
        let feature_straddles = morphs
            .chunk_by(|x, y| x.feature_index == y.feature_index)
            .filter(|units| units.len() == 1)
            .any(|units| straddles(&units[0]));
        if is_rejected || feature_straddles {
            continue;
        }
        match accepted.last_mut() {
            Some(last) if start < last.1 => last.1 = last.1.max(end),
            _ => accepted.push((start, end)),
        }
    }
    accepted
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::{KaomojiPart, find_kaomoji_parts};

    /// 期待値はpyopenjtalk-plus v0.4.1-post10の`find_kaomoji_parts`の出力。
    #[rstest]
    #[case("(´・ω・`)ノ", &[(0, 7, 0, 8)])]
    #[case("(^_^)", &[(0, 5, 0, 5)])]
    #[case("(-'ω')", &[(0, 6, 0, 6)])]
    #[case("(-ノ_ノ)", &[(0, 6, 0, 6)])]
    #[case("(T_T)", &[(0, 5, 0, 5)])]
    #[case("(0_0)", &[(0, 5, 0, 5)])]
    #[case("(⊃-⊂)", &[(0, 5, 0, 5)])]
    #[case("(ﾉ)ω(ヾ)", &[(0, 7, 0, 7)])]
    #[case("(*>ω<)ω<*)", &[(0, 10, 0, 10)])]
    #[case("(^ω^)人(^ω^)", &[(0, 11, 0, 11)])]
    #[case("(^o^)人(^o^)", &[(0, 11, 0, 11)])]
    #[case("(^ω^人^ω^)", &[(0, 9, 0, 9)])]
    #[case("(^ω^)σ)ω^)", &[(0, 10, 0, 10)])]
    #[case("(^ω^)っ)ω^)", &[(0, 10, 0, 10)])]
    #[case("(^ω^)σ\")ω^)", &[(0, 11, 0, 11)])]
    #[case("(ﾉｼ｀･ω･)", &[(0, 8, 0, 8)])]
    #[case("(^ω^)ω^)ω^)", &[(0, 11, 0, 11)])]
    #[case("(๑′ฅฅ‵๑)", &[(0, 8, 0, 8)])]
    #[case("(´∩′•ω•∩)", &[(0, 9, 0, 9)])]
    #[case("(^_^)つー○○○", &[(0, 5, 0, 10)])]
    #[case("(^_^)つ☕\u{fe0f}", &[(0, 5, 0, 8)])]
    #[case("(^_^)つ︻┳═一", &[(0, 5, 0, 10)])]
    #[case("(^_^)つー[二二]", &[(0, 5, 0, 11)])]
    #[case("(^_^)つー{}@{}", &[(0, 5, 0, 12)])]
    #[case("(^_^)つ■D", &[(0, 5, 0, 8)])]
    #[case("(^_^)つー>ﾟ)))≫彡~~", &[(0, 5, 0, 16)])]
    #[case("コ(^_^)ーヒー", &[(1, 6, 1, 6)])]
    #[case("前(^_^) (T_T)後", &[(1, 6, 1, 6), (7, 12, 7, 12)])]
    #[case("m(_ _)m", &[(1, 6, 0, 7)])]
    #[case("ｍ（＿　＿）ｍ", &[(1, 6, 0, 7)])]
    #[case("答えは10 m(_ _)m。", &[(7, 12, 6, 13)])]
    #[case("一(^_^)々", &[(1, 6, 1, 6)])]
    #[case("仲良ｼ(^_^)", &[(3, 8, 3, 8)])]
    #[case("pc(^_^)です", &[(2, 7, 2, 7)])]
    #[case("(^_^)ノート", &[(0, 5, 0, 6)])]
    #[case("(^_^)つー○○○円", &[(0, 5, 0, 6)])]
    #[case("(^_^)つー☆5個", &[(0, 5, 0, 6)])]
    #[case("(^ω^)3人(^ω^)", &[(0, 5, 0, 5), (7, 12, 7, 12)])]
    #[case("「_(┐「ε:)_」", &[(2, 8, 1, 9)])]
    #[case("前(^_^)\u{301}後", &[(1, 6, 1, 7)])]
    #[case("やっほー！元気かな？ヾ(≧▽≦)ﾉ", &[(11, 16, 10, 17)])]
    #[case("(:3[___]", &[(0, 8, 0, 8)])]
    #[case("(¦3[▓▓]", &[(0, 7, 0, 7)])]
    #[case(">ω<", &[(0, 3, 0, 3)])]
    #[case("/ᐠ｡ꞈ｡ᐟ\\", &[(0, 7, 0, 7)])]
    #[case("（・∀・）", &[(0, 5, 0, 5)])]
    #[case("ヽ(*´∀｀)ﾉ", &[(1, 7, 0, 8)])]
    #[case("(｀・ω・´)ゞ", &[(0, 7, 0, 8)])]
    #[case("ʕ•ᴥ•ʔ", &[(0, 5, 0, 5)])]
    #[case("(*´ω｀*)", &[(0, 7, 0, 7)])]
    #[case("_(:3」∠)_", &[(1, 7, 0, 8)])]
    #[case("(´；ω；｀)", &[(0, 7, 0, 7)])]
    #[case("(^^)\n(^^) ♪", &[(0, 4, 0, 4), (5, 9, 5, 9)])]
    #[case("\u{200b}(ﾉ)ω(ヾ)", &[(1, 8, 1, 8)])]
    // 顔文字ではないもの
    #[case("(p-q)", &[])]
    #[case("(a+b)", &[])]
    #[case("(2026年)", &[])]
    #[case("(ﾉ)日本語(ヾ)", &[])]
    #[case("(pH)", &[])]
    #[case("(NaCl)", &[])]
    #[case("f(x)", &[])]
    #[case("(笑)", &[])]
    #[case("ゝ", &[])]
    #[case("(^^)\u{200d}", &[])]
    #[case("普通の文です。", &[])]
    fn find_kaomoji_parts_works(
        #[case] text: &str,
        #[case] expected: &[(usize, usize, usize, usize)],
    ) {
        let expected = expected
            .iter()
            .map(|&(core_start, core_end, start, end)| KaomojiPart {
                core_start,
                core_end,
                start,
                end,
            })
            .collect::<Vec<_>>();
        assert_eq!(expected, find_kaomoji_parts(text));
    }
}
