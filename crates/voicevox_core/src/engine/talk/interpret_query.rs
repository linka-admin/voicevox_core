//! [`AudioQuery`]から特徴量を取り出す処理を集めたもの。

use std::{num::Saturating, ops::Add};

use itertools::chain;
use typed_floats::{NonNaNFinite, PositiveFinite, tf32};

use crate::{
    AccentPhrase, AudioQuery, Mora,
    numerics::{non_nan_finite_f32, positive_finite_f32},
};

use super::{
    super::{
        PhonemeCode,
        acoustic_feature_extractor::{MoraTail, OptionalConsonant},
        frame::FRAME_RATE,
        talk::{LengthedPhoneme, ValidatedAccentPhrase, ValidatedAudioQuery, ValidatedMora},
    },
    full_context_label::mora_to_text,
};

pub const DEFAULT_ENABLE_INTERROGATIVE_UPSPEAK: bool = true;

const FIX_VOWEL_LENGTH: PositiveFinite<f32> = positive_finite_f32!(0.15);

/// [`InterrogativeUpspeakStyle::Glide`]で最後のモーラの母音を伸ばす長さ。
const GLIDE_EXTRA_VOWEL_LENGTH: PositiveFinite<f32> = positive_finite_f32!(0.06);

/// 疑問文の語尾の音高の上げ方。
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum InterrogativeUpspeakStyle {
    /// 最後のモーラの後ろに、音高を上げた母音のモーラを`0.15`秒追加する。
    ///
    /// VOICEVOX ENGINEと同じ挙動。
    AppendMora,

    /// モーラを追加せず、最後のモーラの母音を`0.06`秒伸ばし、その母音の中で音高をなめらかに上げる。
    #[default]
    Glide,
}

pub(crate) fn initial_process<'query>(
    accent_phrases: &[ValidatedAccentPhrase<'query>],
) -> (Vec<ValidatedMora<'query>>, Vec<PhonemeCode>) {
    let flatten_moras = to_flatten_moras(accent_phrases);

    let mut phoneme_data_list = vec![PhonemeCode::MorablePau];
    for mora in flatten_moras.iter() {
        if let Some(consonant) = &mora.consonant {
            phoneme_data_list.push(consonant.phoneme.into())
        }
        phoneme_data_list.push(mora.vowel.phoneme.clone().into());
    }
    phoneme_data_list.push(PhonemeCode::MorablePau);

    return (flatten_moras, phoneme_data_list);

    fn to_flatten_moras<'query>(
        accent_phrases: &[ValidatedAccentPhrase<'query>],
    ) -> Vec<ValidatedMora<'query>> {
        let mut flatten_moras = Vec::new();

        for ValidatedAccentPhrase {
            moras, pause_mora, ..
        } in accent_phrases
        {
            for mora in moras {
                flatten_moras.push(mora.clone());
            }
            if let Some(pause_mora) = pause_mora {
                flatten_moras.push(pause_mora.clone());
            }
        }

        flatten_moras
    }
}

pub(crate) fn split_mora(
    phoneme_list: &[PhonemeCode],
) -> (Vec<OptionalConsonant>, Vec<MoraTail>, Vec<i64>) {
    let mut vowel_phoneme_list = Vec::new();
    let mut vowel_indexes = Vec::new();
    for (i, phoneme) in phoneme_list.iter().enumerate() {
        if let Ok(mora_tail) = (*phoneme).try_into() {
            vowel_phoneme_list.push(mora_tail);
            vowel_indexes.push(i as i64);
        }
    }

    let mut consonant_phoneme_list = vec![OptionalConsonant::None];
    for i in 0..(vowel_indexes.len() - 1) {
        let prev = vowel_indexes[i];
        let next = vowel_indexes[i + 1];
        if next - prev == 1 {
            consonant_phoneme_list.push(OptionalConsonant::None);
        } else {
            consonant_phoneme_list.push(
                phoneme_list[next as usize - 1]
                    .try_into()
                    .expect("`OptionalConsonant` and `MoraTail` should be exclusive"),
            );
        }
    }

    (consonant_phoneme_list, vowel_phoneme_list, vowel_indexes)
}

impl AudioQuery {
    /// 音声の総フレーム数を算出する。
    ///
    /// 音声の秒数は、フレーム数を[`FRAME_RATE`]で割った値で表せる。
    ///
    /// 算出方法は以下の通り。
    ///
    /// 1. 32-bit浮動小数点数の値として存在する以下の秒数を集める。
    ///     - [`AudioQuery::pre_phoneme_length`]
    ///     - [`AudioQuery::accent_phrases`]の要素ごとに
    ///         - [`AccentPhrase::moras`]の要素ごとに
    ///             - [`Mora::consonant_length`]
    ///             - [`Mora::vowel_length`]（ただし後述の条件で[`InterrogativeUpspeakStyle::Glide`]のとき、最後のモーラは`0.06`秒足したもの）
    ///         - [`enable_interrogative_upspeak`]かつ[`AccentPhrase::is_interrogative`]かつ[`AccentPhrase::moras`]の最後の[`Mora::pitch`]が`0.0`以外で、[`InterrogativeUpspeakStyle::AppendMora`]のとき、`0.15`秒
    ///         - [`AccentPhrase::pause_mora`]の[`Mora::consonant_length`]（通常はない）
    ///         - [`AccentPhrase::pause_mora`]の[`Mora::vowel_length`]
    ///     - [`AudioQuery::post_phoneme_length`]
    /// 2. それぞれの秒数を`secs`として、対応するフレーム長を<code>((secs * [FRAME_RATE]).[round_ties_even()] / [speed_scale]).[round_ties_even()]</code>として算出する。
    /// 3. 各フレーム長を足し合わせる。
    ///
    /// # Caveats
    ///
    /// `AudioQuery`に対応する音声の長さは将来的に変わる可能性がある。例えば、秒数を64-bit浮動小数点数として解釈しているVOICEVOX
    /// ENGINEと挙動を揃える可能性がある。
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> anyhow::Result<()> {
    /// # use pollster::FutureExt as _;
    /// # use voicevox_core::{__internal::doctest_fixtures::IntoBlocking as _, StyleId};
    /// #
    /// # const WHATEVER_STYLE1: StyleId = StyleId(0);
    /// # const WHATEVER_STYLE2: StyleId = StyleId(302);
    /// #
    /// # let synth =
    /// #     voicevox_core::__internal::doctest_fixtures::synthesizer_with_sample_voice_model(
    /// #         test_util::SAMPLE_VOICE_MODEL_FILE_PATH,
    /// #         test_util::ONNXRUNTIME_DYLIB_PATH,
    /// #         test_util::OPEN_JTALK_DIC_DIR,
    /// #     )
    /// #     .block_on()?
    /// #     .into_blocking();
    /// #
    /// let query =
    ///     &synth.create_audio_query("こんにちは、音声合成の世界へようこそ？", WHATEVER_STYLE1)?;
    /// let audio = synth
    ///     .create_audio_feature(query, WHATEVER_STYLE2)
    ///     .perform()?;
    ///
    /// assert_eq!(audio.frame_length(), query.frame_length().calculate().0);
    /// #
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// # use voicevox_core::AudioQuery;
    /// #
    /// use voicevox_core::FRAME_RATE;
    ///
    /// let mut query = AudioQuery::from(vec![]);
    /// query.speed_scale = typed_floats::as_const!(PositiveFinite, f32, 1.2);
    /// query.pre_phoneme_length = typed_floats::as_const!(PositiveFinite, f32, 3.3);
    /// query.post_phoneme_length = typed_floats::as_const!(PositiveFinite, f32, 4.4);
    ///
    /// assert_eq!(
    ///     // `speed_scale`, `pre_phoneme_length`
    ///     to_frame_length(3.3, 1.2)
    ///         // `speed_scale`, `consonant_length`, `vowel_length`, `is_interrogative`
    ///         + 0
    ///         // `speed_scale`, `post_phoneme_length`
    ///         + to_frame_length(4.4, 1.2),
    ///     query.frame_length().calculate().0,
    /// );
    ///
    /// fn to_frame_length(secs: f32, speed_scale: f32) -> usize {
    ///     ((secs * FRAME_RATE as f32).round_ties_even() / speed_scale).round_ties_even() as _
    /// }
    /// ```
    ///
    /// ```
    /// # use voicevox_core::AudioQuery;
    /// #
    /// use typed_floats::tf32;
    ///
    /// let mut query = AudioQuery::from(vec![]);
    /// query.speed_scale = tf32::MIN_POSITIVE.into();
    /// assert_eq!(usize::MAX, query.frame_length().calculate().0);
    /// ```
    ///
    /// [`enable_interrogative_upspeak`]: AudioQueryFrameLength::enable_interrogative_upspeak
    /// [round_ties_even()]: f32::round_ties_even
    /// [speed_scale]: Self::speed_scale
    #[cfg_attr(doc, doc(alias = "voicevox_audio_query_frame_length"))]
    pub fn frame_length(&self) -> AudioQueryFrameLength<'_> {
        AudioQueryFrameLength {
            audio_query: self,
            enable_interrogative_upspeak: DEFAULT_ENABLE_INTERROGATIVE_UPSPEAK,
            interrogative_upspeak_style: Default::default(),
        }
    }
}

/// [`AudioQuery::frame_length`]のビルダー。
#[must_use = "this is a builder. it does nothing until `calculate`d"]
#[derive(Debug)]
pub struct AudioQueryFrameLength<'a> {
    audio_query: &'a AudioQuery,
    enable_interrogative_upspeak: bool,
    interrogative_upspeak_style: InterrogativeUpspeakStyle,
}

impl AudioQueryFrameLength<'_> {
    pub fn enable_interrogative_upspeak(mut self, enable_interrogative_upspeak: bool) -> Self {
        self.enable_interrogative_upspeak = enable_interrogative_upspeak;
        self
    }

    /// 疑問文の語尾の音高の上げ方。[`enable_interrogative_upspeak`]が`true`のときのみ有効。
    ///
    /// [`enable_interrogative_upspeak`]: Self::enable_interrogative_upspeak
    pub fn interrogative_upspeak_style(mut self, style: InterrogativeUpspeakStyle) -> Self {
        self.interrogative_upspeak_style = style;
        self
    }

    /// 音声の総フレーム数を算出する。
    ///
    /// 詳細は[`AudioQuery::frame_length`]を参照。
    pub fn calculate(self) -> Saturating<usize> {
        return chain!(
            [self.audio_query.pre_phoneme_length],
            self.audio_query.accent_phrases.iter().flat_map(
                |AccentPhrase {
                     moras,
                     pause_mora,
                     is_interrogative,
                     ..
                 }| {
                    let upspeak = (self.enable_interrogative_upspeak
                        && *is_interrogative
                        && moras.last().is_some_and(|Mora { pitch, .. }| *pitch != 0.0))
                    .then_some(self.interrogative_upspeak_style);
                    let mut mora_lengths = lengths(moras).collect::<Vec<_>>();
                    if upspeak == Some(InterrogativeUpspeakStyle::Glide)
                        && let Some(last_vowel) = mora_lengths.last_mut()
                    {
                        *last_vowel = extend_glide_vowel(*last_vowel);
                    }
                    chain!(
                        mora_lengths,
                        (upspeak == Some(InterrogativeUpspeakStyle::AppendMora))
                            .then_some(FIX_VOWEL_LENGTH),
                        lengths(pause_mora.as_ref()),
                    )
                },
            ),
            [self.audio_query.post_phoneme_length],
        )
        .map(|length| to_frame_length(length.get(), self.audio_query.speed_scale.get()))
        .map(Saturating)
        .fold(Saturating(0), Add::add); // TODO: Rust 1.91以降なら`Sum`を使える

        fn lengths<'a>(
            moras: impl IntoIterator<Item = &'a Mora>,
        ) -> impl Iterator<Item = PositiveFinite<f32>> {
            moras.into_iter().flat_map(
                |&Mora {
                     consonant_length,
                     vowel_length,
                     ..
                 }| itertools::chain(consonant_length, [vowel_length]),
            )
        }
    }
}

pub(crate) struct DecoderFeature {
    pub(crate) f0: Vec<f32>,
    pub(crate) phoneme: Vec<[f32; PhonemeCode::num_phoneme()]>,
}

impl ValidatedAudioQuery<'_> {
    pub(crate) fn decoder_feature(
        &self,
        interrogative_upspeak: Option<InterrogativeUpspeakStyle>,
    ) -> DecoderFeature {
        let ValidatedAudioQuery {
            accent_phrases,
            speed_scale,
            pitch_scale,
            intonation_scale,
            pre_phoneme_length,
            post_phoneme_length,
            ..
        } = self;

        // FIXME: 可能な範囲でtyped_floatsを取り回し続けるべきではないか？
        let speed_scale = f32::from(*speed_scale);
        let pitch_scale = f32::from(*pitch_scale);
        let intonation_scale = f32::from(*intonation_scale);
        let pre_phoneme_length = f32::from(*pre_phoneme_length);
        let post_phoneme_length = f32::from(*post_phoneme_length);

        let accent_phrases = &match interrogative_upspeak {
            Some(style) => adjust_interrogative_accent_phrases(accent_phrases, style),
            None => accent_phrases.to_owned(),
        };

        let (flatten_moras, phoneme_data_list) = initial_process(accent_phrases);

        // 各モーラの母音の終わりで到達する音高。`None`なら音高は一定。
        let mut glide_end_list = vec![None];
        glide_end_list.extend(accent_phrases.iter().flat_map(
            |ValidatedAccentPhrase {
                 moras,
                 pause_mora,
                 is_interrogative,
                 ..
             }| {
                let glide = interrogative_upspeak == Some(InterrogativeUpspeakStyle::Glide)
                    && *is_interrogative;
                moras
                    .iter()
                    .enumerate()
                    .map(move |(i, mora)| {
                        (glide && i == moras.len() - 1 && mora.pitch != 0.0)
                            .then(|| f32::from(raise_interrogative_pitch(mora.pitch)))
                    })
                    .chain(pause_mora.as_ref().map(|_| None))
            },
        ));
        glide_end_list.push(None);

        let mut phoneme_length_list = vec![pre_phoneme_length];
        let mut f0_list = vec![0.];
        let mut voiced_list = vec![false];
        {
            let mut sum_of_f0_bigger_than_zero = 0.;
            let mut count_of_f0_bigger_than_zero = 0;

            for ValidatedMora {
                consonant,
                vowel,
                pitch,
                ..
            } in flatten_moras
            {
                if let Some(consonant) = consonant {
                    phoneme_length_list.push(consonant.length.into());
                }
                phoneme_length_list.push(vowel.length.into());

                let f0_single = f32::from(pitch) * 2.0_f32.powf(pitch_scale);
                f0_list.push(f0_single);

                let bigger_than_zero = f0_single > 0.;
                voiced_list.push(bigger_than_zero);

                if bigger_than_zero {
                    sum_of_f0_bigger_than_zero += f0_single;
                    count_of_f0_bigger_than_zero += 1;
                }
            }
            phoneme_length_list.push(post_phoneme_length);
            f0_list.push(0.);
            voiced_list.push(false);
            let mean_f0 = sum_of_f0_bigger_than_zero / (count_of_f0_bigger_than_zero as f32);

            for glide_end in glide_end_list.iter_mut().flatten() {
                *glide_end *= 2.0_f32.powf(pitch_scale);
            }

            if !mean_f0.is_nan() {
                for i in 0..f0_list.len() {
                    if voiced_list[i] {
                        f0_list[i] = (f0_list[i] - mean_f0) * intonation_scale + mean_f0;
                    }
                }
                for glide_end in glide_end_list.iter_mut().flatten() {
                    *glide_end = (*glide_end - mean_f0) * intonation_scale + mean_f0;
                }
            }
        }

        let (_, _, vowel_indexes) = split_mora(&phoneme_data_list);

        let mut phoneme = Vec::new();
        let mut f0: Vec<f32> = Vec::new();
        {
            let mut sum_of_phoneme_length = 0;
            let mut count_of_f0 = 0;
            let mut vowel_indexes_index = 0;

            for (i, phoneme_length) in phoneme_length_list.iter().enumerate() {
                let phoneme_length = to_frame_length(*phoneme_length, speed_scale);
                let phoneme_id = usize::from(phoneme_data_list[i]);

                for _ in 0..phoneme_length {
                    let mut phonemes_vec = [0.; _];
                    phonemes_vec[phoneme_id] = 1.;
                    phoneme.push(phonemes_vec)
                }
                sum_of_phoneme_length += phoneme_length;

                if i as i64 == vowel_indexes[vowel_indexes_index] {
                    let start = f0_list[count_of_f0];
                    match glide_end_list[count_of_f0] {
                        Some(end) if phoneme_length > 0 => {
                            // 子音の間は一定で、母音の間に`end`まで線形に上げる
                            let flat = sum_of_phoneme_length - phoneme_length;
                            f0.extend(std::iter::repeat_n(start, flat));
                            f0.extend(
                                (1..=phoneme_length).map(|k| {
                                    start + (end - start) * k as f32 / phoneme_length as f32
                                }),
                            );
                        }
                        _ => f0.extend(std::iter::repeat_n(start, sum_of_phoneme_length)),
                    }
                    count_of_f0 += 1;
                    sum_of_phoneme_length = 0;
                    vowel_indexes_index += 1;
                }
            }
        }
        return DecoderFeature { f0, phoneme };

        fn adjust_interrogative_accent_phrases<'query>(
            accent_phrases: &[ValidatedAccentPhrase<'query>],
            style: InterrogativeUpspeakStyle,
        ) -> Vec<ValidatedAccentPhrase<'query>> {
            accent_phrases
                .iter()
                .map(|accent_phrase| ValidatedAccentPhrase {
                    moras: adjust_interrogative_moras(accent_phrase, style),
                    ..accent_phrase.clone()
                })
                .collect()
        }

        fn adjust_interrogative_moras<'query>(
            ValidatedAccentPhrase {
                moras,
                is_interrogative,
                ..
            }: &ValidatedAccentPhrase<'query>,
            style: InterrogativeUpspeakStyle,
        ) -> Vec<ValidatedMora<'query>> {
            let mut moras = moras.clone();
            if *is_interrogative
                && let Some(last_mora) = moras.last_mut()
                && last_mora.pitch != 0.0
            {
                match style {
                    InterrogativeUpspeakStyle::AppendMora => {
                        let interrogative_mora = make_interrogative_mora(last_mora);
                        moras.push(interrogative_mora);
                    }
                    InterrogativeUpspeakStyle::Glide => {
                        last_mora.vowel.length = extend_glide_vowel(last_mora.vowel.length);
                    }
                }
            }
            moras
        }

        fn make_interrogative_mora<'query>(
            last_mora: &ValidatedMora<'query>,
        ) -> ValidatedMora<'query> {
            let pitch = raise_interrogative_pitch(last_mora.pitch);

            ValidatedMora {
                text: mora_to_text(None, &last_mora.vowel.phoneme.to_string()).into(),
                consonant: None,
                vowel: LengthedPhoneme {
                    phoneme: last_mora.vowel.phoneme.clone(),
                    length: FIX_VOWEL_LENGTH,
                },
                pitch,
            }
        }
    }
}

fn extend_glide_vowel(length: PositiveFinite<f32>) -> PositiveFinite<f32> {
    PositiveFinite::try_from(length + GLIDE_EXTRA_VOWEL_LENGTH).unwrap_or_else(|_| tf32::MAX.into())
}

fn raise_interrogative_pitch(pitch: NonNaNFinite<f32>) -> NonNaNFinite<f32> {
    const ADJUST_PITCH: NonNaNFinite<f32> = non_nan_finite_f32!(0.3);
    const MAX_PITCH: NonNaNFinite<f32> = non_nan_finite_f32!(6.5);

    NonNaNFinite::try_from(pitch + ADJUST_PITCH)
        .unwrap_or_else(|_| tf32::MAX.into())
        .min(MAX_PITCH)
}

fn to_frame_length(secs: f32, speed_scale: f32) -> usize {
    // VOICEVOX ENGINEと挙動を合わせるため、四捨五入ではなく偶数丸めをする
    //
    // https://github.com/VOICEVOX/voicevox_engine/issues/552
    ((secs * FRAME_RATE as f32).round_ties_even() / speed_scale).round_ties_even() as _
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::AudioQuery;

    use super::{DecoderFeature, InterrogativeUpspeakStyle};

    /// 「ノ」で終わる2モーラのアクセント句。
    fn query(is_interrogative: bool, last_pitch: f32) -> AudioQuery {
        let mora = |text: &str, consonant: Option<&str>, pitch: f32| -> Value {
            json!({
                "text": text,
                "consonant": consonant,
                "consonant_length": consonant.map(|_| 0.05),
                "vowel": "o",
                "vowel_length": 0.1,
                "pitch": pitch,
            })
        };
        serde_json::from_value(json!({
            "accent_phrases": [
                {
                    "moras": [mora("ソ", Some("s"), 5.5), mora("ノ", Some("n"), last_pitch)],
                    "accent": 2,
                    "is_interrogative": is_interrogative,
                }
            ],
            "speedScale": 1.0,
            "pitchScale": 0.0,
            "intonationScale": 1.0,
            "volumeScale": 1.0,
            "prePhonemeLength": 0.1,
            "postPhonemeLength": 0.1,
            "outputSamplingRate": 24000,
            "outputStereo": false
        }))
        .unwrap()
    }

    fn feature(query: &AudioQuery, upspeak: Option<InterrogativeUpspeakStyle>) -> DecoderFeature {
        query.to_validated().unwrap().decoder_feature(upspeak)
    }

    /// 最後の有声フレームの区間（後続の無音区間を除く）。
    fn last_voiced_run(f0: &[f32]) -> &[f32] {
        let end = f0.iter().rposition(|&f| f > 0.).unwrap() + 1;
        let start = f0[..end]
            .iter()
            .rposition(|&f| f <= 0.)
            .map_or(0, |i| i + 1);
        &f0[start..end]
    }

    #[test]
    fn append_mora_adds_frames() {
        let q = query(true, 5.8);
        assert!(
            feature(&q, Some(InterrogativeUpspeakStyle::AppendMora))
                .f0
                .len()
                > feature(&q, None).f0.len()
        );
    }

    #[test]
    fn glide_extends_only_the_last_vowel() {
        let q = query(true, 5.8);
        let DecoderFeature { f0, phoneme } = feature(&q, Some(InterrogativeUpspeakStyle::Glide));
        let base = feature(&q, None);

        // 母音0.1秒(9フレーム) → 0.16秒(15フレーム)
        assert_eq!(base.f0.len() + 6, f0.len());
        assert_eq!(f0.len(), phoneme.len());
        let last_vowel_frames = |phoneme: &[[f32; _]]| {
            let post = phoneme
                .iter()
                .rposition(|p| p != phoneme.last().unwrap())
                .unwrap();
            phoneme[..=post]
                .iter()
                .rev()
                .take_while(|p| *p == &phoneme[post])
                .count()
        };
        assert_eq!(9, last_vowel_frames(&base.phoneme));
        assert_eq!(15, last_vowel_frames(&phoneme));
    }

    #[test]
    fn glide_raises_pitch_within_last_vowel() {
        let q = query(true, 5.8);
        let glide = feature(&q, Some(InterrogativeUpspeakStyle::Glide)).f0;

        let run = last_voiced_run(&glide);
        assert!(run.windows(2).all(|w| w[0] <= w[1]), "{run:?}");
        assert!((run.last().unwrap() - (5.8 + 0.3)).abs() < 1e-4, "{run:?}");
        // 子音(5フレーム)の間は元の音高で、母音(15フレーム)の間に上がる
        let last_mora = &run[run.iter().take_while(|&&f| f == 5.5).count()..];
        let flat = last_mora.iter().take_while(|&&f| f == 5.8).count();
        assert_eq!((5, 15), (flat, last_mora.len() - flat), "{run:?}");
    }

    #[test]
    fn frame_length_matches_decoder_feature() {
        for (q, style) in itertools::iproduct!(
            [query(true, 5.8), query(true, 0.), query(false, 5.8)],
            [
                InterrogativeUpspeakStyle::AppendMora,
                InterrogativeUpspeakStyle::Glide
            ]
        ) {
            assert_eq!(
                feature(&q, Some(style)).f0.len(),
                q.frame_length()
                    .interrogative_upspeak_style(style)
                    .calculate()
                    .0,
                "{style:?}",
            );
        }
    }

    #[test]
    fn glide_does_nothing_for_non_interrogative_or_unvoiced_last_mora() {
        for q in [query(false, 5.8), query(true, 0.)] {
            assert_eq!(
                feature(&q, None).f0,
                feature(&q, Some(InterrogativeUpspeakStyle::Glide)).f0
            );
        }
    }
}
